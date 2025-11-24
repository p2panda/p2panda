// SPDX-License-Identifier: MIT OR Apache-2.0

//! Example chat application using p2panda-net.
use std::collections::HashMap;
use std::convert::Infallible;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;
use clap::Parser;
use p2panda_core::{Body, Hash, Header, Operation, PrivateKey};
use p2panda_discovery::address_book::AddressBookStore;
use p2panda_discovery::address_book::memory::MemoryStore as AddressBookMemoryStore;
use p2panda_net_next::utils::{ShortFormat, from_public_key};
use p2panda_net_next::{Network, NetworkBuilder, NodeId, NodeInfo, TopicId, TrustedTransportInfo};
use p2panda_store::{MemoryStore, OperationStore};
use p2panda_sync::TopicSyncManager;
use p2panda_sync::log_sync::Logs;
use p2panda_sync::managers::topic_sync_manager::TopicSyncManagerConfig;
use p2panda_sync::topic_log_sync::TopicLogMap;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha20Rng;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::mpsc;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::prelude::*;

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
pub struct ChatTopicMap(pub HashMap<TopicId, Logs<LogId>>);

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
    let log_id: u64 = rand::rng().random();
    let mut logs = HashMap::new();
    logs.insert(private_key.public_key(), vec![log_id]);

    let endpoint_addr = iroh::EndpointAddr::new(from_public_key(private_key.public_key()));
    let endpoint_addr = endpoint_addr.with_relay_url(RELAY_URL.parse().unwrap());
    let transport_info: TrustedTransportInfo = endpoint_addr.into();

    println!("network id: {}", NETWORK_ID.fmt_short());
    println!("public key: {}", private_key.public_key().to_hex());
    println!("relay url: {}", RELAY_URL);
    println!("node info: {}", json!(transport_info));

    let mut store = ChatStore::new();
    let mut topic_map = ChatTopicMap::default();
    topic_map.0.insert(TOPIC_ID, logs);
    let sync_config = TopicSyncManagerConfig {
        topic_map,
        store: store.clone(),
    };

    let rng: ChaCha20Rng = ChaCha20Rng::from_seed(seed);
    let address_book = AddressBookMemoryStore::<ChaCha20Rng, NodeId, NodeInfo>::new(rng.clone());

    if let Some(id) = args.bootstrap_id {
        if let Some(transport_info) = args.bootstrap_info {
            let transports: TrustedTransportInfo = serde_json::from_str(&transport_info)?;
            let mut node_info = NodeInfo::new(id);
            node_info.update_transports(transports.into())?;
            address_book.insert_node_info(node_info).await?;
        }
    }

    let builder = NetworkBuilder::new(NETWORK_ID);
    let builder = builder.private_key(private_key.clone());
    let builder = builder.relay(RELAY_URL.parse().unwrap());

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

    let mut seq_num = 0;
    let mut backlink = None;

    // Sign and encode each line of text input and broadcast it on the chat topic.
    while let Some(text) = line_rx.recv().await {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let body = Body::new(text.as_bytes());
        let (hash, header, header_bytes, operation) =
            create_operation(&private_key, &body, seq_num, timestamp, backlink);
        store
            .insert_operation(hash, &header, Some(&body), &header_bytes, &log_id)
            .await
            .unwrap();

        stream.publish(operation).await.unwrap();

        seq_num += 1;
        backlink = Some(hash);
    }

    // Listen for `Ctrl+c` and shutdown the node.
    tokio::signal::ctrl_c().await?;

    Ok(())
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

fn create_operation(
    private_key: &PrivateKey,
    body: &Body,
    seq_num: u64,
    timestamp: u64,
    backlink: Option<Hash>,
) -> (
    Hash,
    Header<ChatExtensions>,
    Vec<u8>,
    Operation<ChatExtensions>,
) {
    let mut header = Header {
        version: 1,
        public_key: private_key.public_key(),
        signature: None,
        payload_size: body.size(),
        payload_hash: Some(body.hash()),
        timestamp,
        seq_num,
        backlink,
        previous: vec![],
        extensions: ChatExtensions,
    };

    header.sign(private_key);
    let header_bytes = header.to_bytes();
    let hash = header.hash();

    let operation = Operation {
        hash,
        header: header.clone(),
        body: Some(body.to_owned()),
    };

    (hash, header, header_bytes, operation)
}
