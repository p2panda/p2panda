// SPDX-License-Identifier: MIT OR Apache-2.0

//! Example chat application using p2panda-net.
use std::collections::HashMap;
use std::convert::Infallible;

use anyhow::{Result, bail};
use clap::Parser;
use p2panda_core::{PrivateKey, PublicKey, Signature};
use p2panda_discovery::address_book::AddressBookStore;
use p2panda_discovery::address_book::memory::MemoryStore as AddressBookMemoryStore;
use p2panda_net_next::utils::ShortFormat;
use p2panda_net_next::{
    Network, NetworkBuilder, NodeId, NodeInfo, TopicId, TransportAddress, TransportInfo,
};
use p2panda_store::MemoryStore;
use p2panda_sync::log_sync::Logs;
use p2panda_sync::managers::topic_sync_manager::TopicSyncManagerConfig;
use p2panda_sync::topic_log_sync::TopicLogMap;
use p2panda_sync::{FromSync, TopicSyncManager};
use rand::{Rng, SeedableRng, random};
use rand_chacha::ChaCha20Rng;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::mpsc;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::prelude::*;
use url::Url;

const TOPIC_ID: [u8; 32] = [1; 32];
const NETWORK_ID: [u8; 32] = [7; 32];
const RELAY_URL: &str = "https://euc1-1.relay.n0.iroh-canary.iroh.link.";

pub fn setup_logging() {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
        .with(EnvFilter::from_default_env())
        .try_init()
        .ok();
}

#[derive(Parser)]
struct Args {
    /// Supply seed for deterministic node id generation.
    #[arg(short = 's', long, value_name = "SEED")]
    seed: Option<u8>,

    /// Supply the node ID of a bootstrap node.
    #[arg(short = 'b', long, value_name = "BOOTSTRAP_ID")]
    bootstrap_id: Option<NodeId>,

    /// Supply the serialized transport info of a bootstrap node.
    #[arg(short = 'i', long, value_name = "BOOTSTRAP_INFO")]
    bootstrap_info: Option<String>,
}

type LogId = u64;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ChatExtensions;

#[derive(Clone, Default, Debug)]
pub struct ChatTopicMap(HashMap<TopicId, Logs<LogId>>);

impl TopicLogMap<TopicId, LogId> for ChatTopicMap {
    type Error = Infallible;

    async fn get(&self, topic_query: &TopicId) -> Result<Logs<LogId>, Self::Error> {
        Ok(self.0.get(topic_query).cloned().unwrap_or_default())
    }
}

type ChatStore = MemoryStore<LogId, ChatExtensions>;

type ChatTopicSyncManager =
    TopicSyncManager<TopicId, ChatStore, ChatTopicMap, LogId, ChatExtensions>;

#[tokio::main]
async fn main() -> Result<()> {
    setup_logging();

    let args = Args::parse();
    let seed = args
        .seed
        .map(|seed| [seed; 32])
        .unwrap_or_else(|| rand::random::<[u8; 32]>());

    let private_key = PrivateKey::from_bytes(&seed);
    let relay_url = Url::parse(RELAY_URL)?;
    let mut info = TransportInfo::new_unsigned();
    info.add_addr(TransportAddress::from_iroh(
        private_key.public_key(),
        Some(relay_url.clone().into()),
        [],
    ));
    let info = info.sign(&private_key)?;

    println!("network id: {}", NETWORK_ID.fmt_short());
    println!("public key: {}", private_key.public_key().to_hex());
    println!("relay url: {}", RELAY_URL);
    println!("node info: {}", json!(info));

    let store = ChatStore::new();
    let topic_map = ChatTopicMap::default();
    let sync_config = TopicSyncManagerConfig { topic_map, store };

    let rng: ChaCha20Rng = ChaCha20Rng::from_seed(seed);
    let address_book = AddressBookMemoryStore::<ChaCha20Rng, NodeId, NodeInfo>::new(rng.clone());

    if let Some(id) = args.bootstrap_id {
        if let Some(transport_info) = args.bootstrap_info {
            let transports: TransportInfo = serde_json::from_str(&transport_info)?;
            let mut node_info = NodeInfo::new(id);
            node_info.update_transports(transports)?;
            address_book.insert_node_info(node_info).await?;
        }
    }

    // let ipv4_port = rng.random_range(2000, 3000);
    // let ipv6_port = rng.random_range(2000, 3000);
    let builder = NetworkBuilder::new(NETWORK_ID);
    let builder = builder.private_key(private_key.clone());
    let builder = builder.relay(relay_url.into());
    // let builder = builder.bind_port_v4(ipv4_port);
    // let builder = builder.bind_port_v6(ipv6_port);

    let network: Network<ChatTopicSyncManager> =
        builder.build(address_book, sync_config).await.unwrap();

    let stream = network.stream(TOPIC_ID, true).await?;
    let mut stream_subscriber = stream.subscribe().await?;

    // Receive messages from the network stream.
    tokio::task::spawn(async move {
        while let Ok(from_sync) = stream_subscriber.recv().await {
            // TODO: Proper match on FromSync<TopicLogSyncEvent<ChatExtensions>>.
            println!("{:?}", from_sync);
        }
    });

    // Listen for text input via the terminal.
    let (line_tx, mut line_rx) = mpsc::channel(1);
    std::thread::spawn(move || input_loop(line_tx));

    // Sign and encode each line of text input and broadcast it on the chat topic.
    while let Some(text) = line_rx.recv().await {
        let bytes = Message::sign_and_encode(&private_key, &text)?;
        stream.publish(bytes).await.ok();
    }

    // Listen for `Ctrl+c` and shutdown the node.
    tokio::signal::ctrl_c().await?;

    Ok(())
}

#[derive(Serialize, Deserialize)]
struct Message {
    id: u32,
    signature: Signature,
    public_key: PublicKey,
    text: String,
}

impl Message {
    pub fn sign_and_encode(private_key: &PrivateKey, text: &str) -> Result<Vec<u8>> {
        // Sign text content.
        let mut text_bytes: Vec<u8> = Vec::new();
        ciborium::ser::into_writer(text, &mut text_bytes)?;
        let signature = private_key.sign(&text_bytes);

        // Encode message.
        let message = Message {
            // Make every message unique, as duplicates get ignored during gossip broadcast.
            id: random(),
            signature,
            public_key: private_key.public_key(),
            text: text.to_owned(),
        };
        let mut bytes: Vec<u8> = Vec::new();
        ciborium::ser::into_writer(&message, &mut bytes)?;

        Ok(bytes)
    }

    fn decode_and_verify(bytes: &[u8]) -> Result<Self> {
        // Decode message.
        let message: Self = ciborium::de::from_reader(bytes)?;

        // Verify signature.
        let mut text_bytes: Vec<u8> = Vec::new();
        ciborium::ser::into_writer(&message.text, &mut text_bytes)?;
        if !message.public_key.verify(&text_bytes, &message.signature) {
            bail!("invalid signature");
        }

        Ok(message)
    }
}

fn input_loop(line_tx: mpsc::Sender<String>) -> Result<()> {
    let mut buffer = String::new();
    let stdin = std::io::stdin();
    loop {
        stdin.read_line(&mut buffer)?;
        line_tx.blocking_send(buffer.clone())?;
        buffer.clear();
    }
}
