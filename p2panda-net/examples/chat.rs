// SPDX-License-Identifier: MIT OR Apache-2.0

//! Example chat application using p2panda-net.
use std::collections::HashMap;
use std::convert::Infallible;
use std::net::{Ipv4Addr, Ipv6Addr};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::Result;
use clap::Parser;
use futures_util::StreamExt;
use iroh::EndpointAddr;
use p2panda_core::{Body, Hash, Header, Operation, PrivateKey};
use p2panda_net::addrs::NodeInfo;
use p2panda_net::discovery::DiscoveryConfig;
use p2panda_net::iroh_endpoint::{IrohConfig, from_public_key};
use p2panda_net::iroh_mdns::MdnsDiscoveryMode;
use p2panda_net::utils::ShortFormat;
use p2panda_net::{
    AddressBook, Discovery, Endpoint, Gossip, LogSync, MdnsDiscovery, NodeId, TopicId,
};
use p2panda_store::MemoryStore;
use p2panda_sync::Logs;
use p2panda_sync::traits::TopicLogMap;
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
    let store = ChatStore::new();

    let topic_map = ChatTopicMap::default();
    topic_map.insert(public_key, LOG_ID).await;

    // Prepare address book.
    let address_book = AddressBook::builder().spawn().await.unwrap();

    // Add a bootstrap node to our address book if one was supplied by the user.
    if let Some(id) = args.bootstrap_id {
        let endpoint_addr = EndpointAddr::new(from_public_key(id));
        let endpoint_addr = endpoint_addr.with_relay_url(RELAY_URL.parse().unwrap());
        address_book
            .insert_node_info(NodeInfo::from(endpoint_addr).bootstrap())
            .await?;
    }

    let endpoint_config = IrohConfig {
        bind_ip_v4: Ipv4Addr::LOCALHOST,
        bind_port_v4: 0,
        bind_ip_v6: Ipv6Addr::LOCALHOST,
        bind_port_v6: 0,
        relay_urls: vec![RELAY_URL.parse().unwrap()],
    };

    let endpoint = Endpoint::builder(address_book.clone())
        .private_key(private_key)
        .network_id(NETWORK_ID)
        .config(endpoint_config)
        .spawn()
        .await
        .unwrap();

    let _discovery = Discovery::builder(address_book.clone(), endpoint.clone())
        .config(DiscoveryConfig::default())
        .spawn()
        .await
        .unwrap();

    if args.mdns {
        let _mdns = MdnsDiscovery::builder(address_book.clone(), endpoint.clone())
            .mode(MdnsDiscoveryMode::Active)
            .spawn()
            .await
            .unwrap();
    }

    let gossip = Gossip::builder(address_book.clone(), endpoint.clone())
        .spawn()
        .await
        .unwrap();

    // Subscribe to gossip overlay to receive (ephemeral) messages.
    let gossip_tx = gossip.stream(HEARTBEAT_TOPIC).await.unwrap();
    let mut gossip_rx = gossip_tx.subscribe();

    // Receive and log each (ephemeral) heartbeat message.
    tokio::spawn(async move {
        loop {
            if let Some(Ok(message)) = gossip_rx.next().await {
                info!(
                    "heartbeat <3 {}",
                    u64::from_be_bytes(message.try_into().unwrap())
                );
            }
        }
    });

    // Publish (ephemeral) heartbeat messages.
    tokio::task::spawn(async move {
        loop {
            let rnd: u64 = rand::random();
            gossip_tx.publish(rnd.to_be_bytes()).await.unwrap();
            tokio::time::sleep(Duration::from_secs(rand::random_range(20..30))).await;
        }
    });

    let sync = LogSync::builder(store, topic_map, address_book, endpoint, gossip)
        .spawn()
        .await
        .unwrap();

    let sync_tx = sync.stream(CHAT_TOPIC, true).await.unwrap();
    let _sync_rx = sync_tx.subscribe().await.unwrap();

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
