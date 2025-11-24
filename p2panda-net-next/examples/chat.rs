// SPDX-License-Identifier: MIT OR Apache-2.0

//! Example chat application using p2panda-net.
use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::Arc;

use anyhow::Result;
use clap::Parser;
use p2panda_core::PrivateKey;
use p2panda_discovery::address_book::memory::MemoryStore as AddressBookMemoryStore;
use p2panda_discovery::address_book::AddressBookStore;
use p2panda_net_next::utils::{from_public_key, ShortFormat};
use p2panda_net_next::{Network, NetworkBuilder, NodeId, NodeInfo, TopicId};
use p2panda_store::MemoryStore;
use p2panda_sync::log_sync::Logs;
use p2panda_sync::managers::topic_sync_manager::TopicSyncManagerConfig;
use p2panda_sync::topic_log_sync::TopicLogMap;
use p2panda_sync::TopicSyncManager;
use rand::SeedableRng;
use rand_chacha::ChaCha20Rng;
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, RwLock};
use tracing_subscriber::prelude::*;
use tracing_subscriber::EnvFilter;

type LogId = u64;

const TOPIC_ID: [u8; 32] = [1; 32];

const NETWORK_ID: [u8; 32] = [7; 32];

const RELAY_URL: &str = "https://euc1-1.relay.n0.iroh-canary.iroh.link.";

const LOG_ID: LogId = 1;

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
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ChatExtensions;

#[derive(Clone, Default, Debug)]
pub struct ChatTopicMap(Arc<RwLock<HashMap<TopicId, Logs<LogId>>>>);

impl ChatTopicMap {
    async fn insert(&self, node_id: NodeId, log_id: LogId) {
        let mut map = self.0.write().await;
        map.entry(TOPIC_ID)
            .and_modify(|logs| {
                logs.insert(node_id, vec![log_id]);
            })
            .or_insert({
                let mut value = HashMap::new();
                value.insert(node_id, vec![log_id]);
                value
            });
    }
}

impl TopicLogMap<TopicId, LogId> for ChatTopicMap {
    type Error = Infallible;

    async fn get(&self, topic_query: &TopicId) -> Result<Logs<LogId>, Self::Error> {
        let map = self.0.read().await;
        Ok(map.get(topic_query).cloned().unwrap_or_default())
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
    let public_key = private_key.public_key();

    let mut logs = HashMap::new();
    logs.insert(public_key, vec![LOG_ID]);

    println!("network id: {}", NETWORK_ID.fmt_short());
    println!("public key: {}", public_key.to_hex());
    println!("relay url: {}", RELAY_URL);

    let store = ChatStore::new();
    let topic_map = ChatTopicMap::default();
    topic_map.insert(public_key, LOG_ID).await;

    let sync_config = TopicSyncManagerConfig {
        topic_map: topic_map.clone(),
        store: store.clone(),
    };

    let rng: ChaCha20Rng = ChaCha20Rng::from_seed(seed);
    let address_book = AddressBookMemoryStore::<ChaCha20Rng, NodeId, NodeInfo>::new(rng.clone());

    if let Some(id) = args.bootstrap_id {
        let endpoint_addr = iroh::EndpointAddr::new(from_public_key(id));
        let endpoint_addr = endpoint_addr.with_relay_url(RELAY_URL.parse().unwrap());
        address_book
            .insert_node_info(NodeInfo::from(endpoint_addr))
            .await?;
    }

    let builder = NetworkBuilder::new(NETWORK_ID);
    let builder = builder.private_key(private_key.clone());
    let builder = builder.relay(RELAY_URL.parse().unwrap());

    let network: Network<ChatTopicSyncManager> =
        builder.build(address_book, sync_config).await.unwrap();

    // Ephemeral stream.
    let ephemeral = network.ephemeral_stream([99; 32]).await?;
    let mut ephemeral_subscriber = ephemeral.subscribe().await?;

    tokio::task::spawn(async move {
        loop {
            let message = ephemeral_subscriber.recv().await.unwrap();
            println!("{}", String::from_utf8(message).unwrap());
        }
    });

    // Listen for text input via the terminal.
    let (line_tx, mut line_rx) = mpsc::channel(1);
    std::thread::spawn(move || input_loop(line_tx));

    while let Some(text) = line_rx.recv().await {
        ephemeral.publish(text.into_bytes()).await.unwrap()
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
