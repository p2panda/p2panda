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
use p2panda_net_next::{Network, NetworkBuilder, NodeId, NodeInfo, TopicId};
use p2panda_store::MemoryStore;
use p2panda_sync::log_sync::Logs;
use p2panda_sync::managers::topic_sync_manager::TopicSyncManagerConfig;
use p2panda_sync::topic_log_sync::TopicLogMap;
use p2panda_sync::{SyncManagerEvent, TopicSyncManager};
use rand::{SeedableRng, random};
use rand_chacha::ChaCha20Rng;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use url::Url;

const TOPIC_ID: [u8; 32] = [1; 32];
const NETWORK_ID: [u8; 32] = [7; 32];
const RELAY_URL: &str = "https://euc1-1.relay.n0.iroh-canary.iroh.link.";

#[derive(Parser)]
struct Args {
    /// Supply the node ID of a bootstrap node for discovery over the internet.
    #[arg(short = 'p', long, value_name = "NODE_ID")]
    bootstrap_node: Option<NodeId>,
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
    let args = Args::parse();

    let private_key = PrivateKey::new();
    let relay_url = Url::parse(RELAY_URL)?;

    println!("network id: {}", NETWORK_ID.fmt_short());
    println!("public key: {}", private_key.public_key().to_hex());
    println!("relay url: {}", RELAY_URL);

    let store = ChatStore::new();
    let topic_map = ChatTopicMap::default();
    let sync_config = TopicSyncManagerConfig { topic_map, store };

    let seed = rand::random();
    let rng = ChaCha20Rng::from_seed(seed);
    let address_book = AddressBookMemoryStore::<ChaCha20Rng, NodeId, NodeInfo>::new(rng.clone());

    if let Some(node_id) = args.bootstrap_node {
        let node_info = NodeInfo {
            node_id,
            bootstrap: true,
            transports: None,
        };
        address_book.insert_node_info(node_info).await?;
    }

    let builder = NetworkBuilder::new(NETWORK_ID);
    let builder = builder.private_key(private_key.clone());
    let builder = builder.relay(relay_url.into());

    let network: Network<ChatTopicSyncManager> =
        builder.build(address_book, sync_config).await.unwrap();

    let stream = network.stream(TOPIC_ID, true).await?;
    let mut stream_subscriber = stream.subscribe().await?;

    // Receive messages from the network stream.
    tokio::task::spawn(async move {
        while let Ok(event) = stream_subscriber.recv().await {
            match event {
                SyncManagerEvent::TopicAgreed { .. } => {
                    print!("agreed on sync topic with remote node");
                }
                SyncManagerEvent::FromSync {
                    session_id: _,
                    event,
                } => {
                    // TODO: Decode message.
                    print!("{:?}", event);
                }
            }
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
