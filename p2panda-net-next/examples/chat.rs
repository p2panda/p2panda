// SPDX-License-Identifier: MIT OR Apache-2.0

//! Example chat application using p2panda-net.
use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::Result;
use clap::Parser;
use p2panda_core::{Body, Hash, Header, Operation, PrivateKey};
use p2panda_discovery::address_book::AddressBookStore;
use p2panda_discovery::address_book::memory::MemoryStore as AddressBookMemoryStore;
use p2panda_net_next::utils::{ShortFormat, from_public_key};
use p2panda_net_next::{MdnsDiscoveryMode, Network, NetworkBuilder, NodeId, NodeInfo, TopicId};
use p2panda_store::{MemoryStore, OperationStore};
use p2panda_sync::TopicSyncManager;
use p2panda_sync::log_sync::Logs;
use p2panda_sync::managers::topic_sync_manager::TopicSyncManagerConfig;
use p2panda_sync::topic_log_sync::{TopicLogMap, TopicLogSyncEvent};
use rand::SeedableRng;
use rand_chacha::ChaCha20Rng;
use tokio::sync::{RwLock, mpsc};
use tracing::info;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::prelude::*;

type LogId = u64;

const CHAT_TOPIC: [u8; 32] = [1; 32];

const HEARTBEAT_TOPIC: [u8; 32] = [2; 32];

const NETWORK_ID: [u8; 32] = [7; 32];

const RELAY_URL: &str = "https://euc1-1.relay.n0.iroh-canary.iroh.link.";

/// This application maintains only one log per author, this is why we can hard-code it.
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
    /// Identifier of a bootstrap node.
    #[arg(short = 'b', long, value_name = "BOOTSTRAP_ID")]
    bootstrap_id: Option<NodeId>,

    /// Seed for deterministic private key generation.
    #[arg(short = 's', long, value_name = "SEED")]
    seed: Option<u8>,

    /// Enable mDNS discovery
    #[arg(short = 'm', long, action)]
    mdns: bool,
}

#[derive(Clone, Default, Debug)]
pub struct ChatTopicMap(Arc<RwLock<HashMap<TopicId, Logs<LogId>>>>);

impl ChatTopicMap {
    async fn insert(&self, node_id: NodeId, log_id: LogId) {
        let mut map = self.0.write().await;
        map.entry(CHAT_TOPIC)
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

type ChatStore = MemoryStore<LogId, ()>;

type ChatTopicSyncManager = TopicSyncManager<TopicId, ChatStore, ChatTopicMap, LogId, ()>;

#[tokio::main]
async fn main() -> Result<()> {
    setup_logging();

    let args = Args::parse();

    // Use manually configured seed or pick a random one.
    let seed = args
        .seed
        .map(|seed| [seed; 32])
        .unwrap_or_else(|| rand::random::<[u8; 32]>());

    let private_key = PrivateKey::from_bytes(&seed);
    let public_key = private_key.public_key();

    println!("network id: {}", NETWORK_ID.fmt_short());
    println!("public key: {}", public_key.to_hex());
    println!("relay url: {}", RELAY_URL);

    // Set up sync for p2panda operations.
    let mut store = ChatStore::new();

    let topic_map = ChatTopicMap::default();
    topic_map.insert(public_key, LOG_ID).await;

    let sync_config = TopicSyncManagerConfig {
        topic_map: topic_map.clone(),
        store: store.clone(),
    };

    // Prepare address book.
    let address_book = {
        let rng = ChaCha20Rng::from_seed(seed);
        AddressBookMemoryStore::<ChaCha20Rng, NodeId, NodeInfo>::new(rng.clone())
    };

    // If user mentioned a bootstrap node, we're adding it here to our address book.
    if let Some(id) = args.bootstrap_id {
        let endpoint_addr = iroh::EndpointAddr::new(from_public_key(id));
        let endpoint_addr = endpoint_addr.with_relay_url(RELAY_URL.parse().unwrap());
        address_book
            .insert_node_info(NodeInfo::from(endpoint_addr))
            .await?;
    }

    // Set up peer-to-peer network and launch it.
    let mut builder = NetworkBuilder::new(NETWORK_ID)
        .private_key(private_key.clone())
        .relay(RELAY_URL.parse().expect("correct relay url"));

    if args.mdns {
        builder = builder.mdns(MdnsDiscoveryMode::Active);
    }

    let network: Network<ChatTopicSyncManager> = builder
        .build(address_book, sync_config)
        .await
        .expect("spawned supervised network");

    // System events.
    let mut events = network.events().await?;
    tokio::task::spawn(async move {
        loop {
            match events.recv().await.unwrap() {
                p2panda_net_next::events::NetworkEvent::Transport(status) => {
                    info!("{:?}", status);
                }
                p2panda_net_next::events::NetworkEvent::Relay(status) => {
                    info!("{:?}", status);
                }
                _ => (),
            }
        }
    });

    // Ephemeral stream.
    let ephemeral = network.ephemeral_stream(HEARTBEAT_TOPIC).await?;
    let mut ephemeral_subscriber = ephemeral.subscribe().await?;

    tokio::task::spawn(async move {
        loop {
            let message = ephemeral_subscriber.recv().await.unwrap();
            info!(
                "heartbeat <3 {}",
                u64::from_be_bytes(message.try_into().unwrap())
            );
        }
    });

    tokio::task::spawn(async move {
        loop {
            let rnd: u64 = rand::random();
            ephemeral.publish(rnd.to_be_bytes()).await.unwrap();
            tokio::time::sleep(Duration::from_secs(rand::random_range(20..30))).await;
        }
    });

    // Eventually consistent stream.
    let stream = network.stream(CHAT_TOPIC, true).await?;
    let mut stream_subscriber = stream.subscribe().await?;

    // Receive messages from the network stream.
    {
        let mut store = store.clone();

        tokio::task::spawn(async move {
            while let Ok(from_sync) = stream_subscriber.recv().await {
                match from_sync.event {
                    TopicLogSyncEvent::SyncFinished(metrics) => {
                        info!(
                            "finished sync session with {}, bytes received = {}, bytes sent = {}",
                            from_sync.remote.fmt_short(),
                            metrics.total_bytes_remote.unwrap_or_default(),
                            metrics.total_bytes_local.unwrap_or_default()
                        );
                    }
                    TopicLogSyncEvent::Operation(operation) => {
                        if store.has_operation(operation.hash).await.unwrap() {
                            continue;
                        }

                        println!(
                            "{}: {}",
                            operation.header.public_key.fmt_short(),
                            String::from_utf8(operation.body.as_ref().unwrap().to_bytes()).unwrap()
                        );

                        store
                            .insert_operation(
                                operation.hash,
                                &operation.header,
                                operation.body.as_ref(),
                                &operation.header.to_bytes(),
                                &LOG_ID,
                            )
                            .await
                            .unwrap();

                        topic_map.insert(operation.header.public_key, LOG_ID).await;
                    }
                    _ => (),
                }
            }
        });
    }

    // Listen for text input via the terminal.
    let (line_tx, mut line_rx) = mpsc::channel(1);
    std::thread::spawn(move || input_loop(line_tx));

    let mut seq_num = 0;
    let mut backlink = None;

    // Sign and encode each line of text input and broadcast it on the chat topic.
    while let Some(text) = line_rx.recv().await {
        let body = Body::new(text.as_bytes());
        let (hash, header, header_bytes, operation) =
            create_operation(&private_key, &body, seq_num, backlink);
        store
            .insert_operation(hash, &header, Some(&body), &header_bytes, &LOG_ID)
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
    backlink: Option<Hash>,
) -> (Hash, Header, Vec<u8>, Operation) {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();

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
        extensions: (),
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
