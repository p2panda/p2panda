// SPDX-License-Identifier: MIT OR Apache-2.0

//! Example chat application using p2panda-net.
use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::Result;
use clap::Parser;
use futures_util::StreamExt;
use iroh::EndpointAddr;
use p2panda_core::{Body, Hash, Header, Operation, PrivateKey, PublicKey};
use p2panda_net::addrs::NodeInfo;
use p2panda_net::discovery::DiscoveryConfig;
use p2panda_net::iroh_endpoint::from_public_key;
use p2panda_net::iroh_mdns::MdnsDiscoveryMode;
use p2panda_net::utils::ShortFormat;
use p2panda_net::{
    AddressBook, Discovery, Endpoint, Gossip, LogSync, MdnsDiscovery, NodeId, TopicId,
};
use p2panda_store::{MemoryStore, OperationStore};
use p2panda_sync::traits::TopicLogMap;
use p2panda_sync::{Logs, TopicLogSyncEvent};
use tokio::sync::{RwLock, mpsc};
use tokio::time::Instant;
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
        .unwrap_or_else(rand::random::<[u8; 32]>);

    let private_key = PrivateKey::from_bytes(&seed);
    let public_key = private_key.public_key();

    println!("network id: {}", NETWORK_ID.fmt_short());
    println!("public key: {}", public_key.to_hex());
    println!("relay url: {}", RELAY_URL);

    // Set up sync for p2panda operations.
    let mut store = ChatStore::new();

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

    let endpoint = Endpoint::builder(address_book.clone())
        .private_key(private_key.clone())
        .network_id(NETWORK_ID)
        .relay_url(RELAY_URL.parse().unwrap())
        .spawn()
        .await
        .unwrap();

    let _discovery = Discovery::builder(address_book.clone(), endpoint.clone())
        .config(DiscoveryConfig::default())
        .spawn()
        .await
        .unwrap();

    let mdns_discovery_mode = if args.mdns {
        MdnsDiscoveryMode::Active
    } else {
        MdnsDiscoveryMode::Disabled
    };
    let _mdns = MdnsDiscovery::builder(address_book.clone(), endpoint.clone())
        .mode(mdns_discovery_mode)
        .spawn()
        .await
        .unwrap();

    let gossip = Gossip::builder(address_book.clone(), endpoint.clone())
        .spawn()
        .await
        .unwrap();

    // Subscribe to gossip overlay to receive (ephemeral) messages.
    let gossip_tx = gossip.stream(HEARTBEAT_TOPIC).await.unwrap();
    let mut gossip_rx = gossip_tx.subscribe();

    // Mapping of public key to nickname.
    let nicknames = Arc::new(RwLock::new(HashMap::<PublicKey, String>::new()));

    // Mapping of public key to the instant that the last heartbeat message was received.
    let status = Arc::new(RwLock::new(HashMap::new()));

    // Receive and log each (ephemeral) heartbeat message.
    {
        let nicknames = Arc::clone(&nicknames);
        let status = Arc::clone(&status);
        tokio::spawn(async move {
            loop {
                if let Some(Ok(message)) = gossip_rx.next().await {
                    // Extract the sender's public key from the message.
                    let (sender, _rnd_bytes) = message.split_at(32);
                    let sender_public_key_bytes: &[u8; 32] = sender.try_into().unwrap();
                    let sender_public_key = PublicKey::from_bytes(sender_public_key_bytes).unwrap();

                    // Look up nickname for sender.
                    let name =
                        if let Some(nickname) = nicknames.read().await.get(&sender_public_key) {
                            nickname.to_owned()
                        } else {
                            sender_public_key.fmt_short()
                        };

                    info!("received heartbeat from {}", name);

                    // Update status hashmap.
                    if status
                        .write()
                        .await
                        .insert(sender_public_key, Instant::now())
                        .is_none()
                    {
                        println!("-> {} came online", name)
                    }
                }
            }
        });
    }

    // TODO: Rather send an explicit "going offline" message before shutdown.
    //
    // Print a message when a peer goes offline.
    {
        let nicknames = Arc::clone(&nicknames);
        let status = Arc::clone(&status);
        tokio::task::spawn(async move {
            loop {
                let now = Instant::now();
                for (public_key, last_heartbeat) in status.read().await.iter() {
                    let secs_since_last_heartbeat = now.duration_since(*last_heartbeat).as_secs();
                    println!(
                        "{} last seen {} seconds ago",
                        public_key, secs_since_last_heartbeat
                    );
                    if secs_since_last_heartbeat > 30 {
                        let name = if let Some(nickname) = nicknames.read().await.get(public_key) {
                            nickname.to_owned()
                        } else {
                            public_key.fmt_short()
                        };

                        println!("-> {} went offline", name);
                    }
                }

                tokio::time::sleep(Duration::from_secs(15)).await;
            }
        });
    }

    // Publish (ephemeral) heartbeat messages.
    tokio::task::spawn(async move {
        loop {
            // Generate a random number to ensure each message is unique.
            let rnd: u64 = rand::random();
            let rnd_bytes = rnd.to_be_bytes().to_vec();

            // Combine our public key with the random number.
            let mut msg = public_key.as_bytes().to_vec();
            msg.extend(rnd_bytes);

            // Publish the message to the gossip topic.
            gossip_tx.publish(msg).await.unwrap();

            tokio::time::sleep(Duration::from_secs(rand::random_range(20..30))).await;
        }
    });

    let sync = LogSync::builder(store.clone(), topic_map.clone(), endpoint, gossip)
        .spawn()
        .await
        .unwrap();

    let sync_tx = sync.stream(CHAT_TOPIC, true).await.unwrap();
    let mut sync_rx = sync_tx.subscribe().await.unwrap();

    // Receive messages from the sync stream.
    {
        let mut store = store.clone();
        let nicknames = Arc::clone(&nicknames);
        tokio::task::spawn(async move {
            while let Some(Ok(from_sync)) = sync_rx.next().await {
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

                        let remote_public_key = operation.header.public_key;
                        let remote_id = remote_public_key.fmt_short();

                        let text =
                            String::from_utf8(operation.body.as_ref().unwrap().to_bytes()).unwrap();

                        // Check if the text of this operation is setting a nickname.
                        if let Some(nick) = text.strip_prefix("/nick ") {
                            if let Some(previous_nick) =
                                nicknames.read().await.get(&remote_public_key)
                            {
                                print!("-> {} is now known as: {}", previous_nick, nick);
                            } else {
                                print!("-> {} is now known as: {}", remote_id, nick);
                            }

                            // Update the nicknames map.
                            nicknames
                                .write()
                                .await
                                .insert(remote_public_key, nick.trim().to_owned());
                        } else {
                            // Print a regular chat message.
                            print!(
                                "{}: {}",
                                nicknames
                                    .read()
                                    .await
                                    .get(&remote_public_key)
                                    .unwrap_or(&remote_id),
                                text
                            )
                        }

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

        sync_tx.publish(operation).await.unwrap();

        seq_num += 1;
        backlink = Some(hash);

        // Update the nickname mapping for the local node.
        if let Some(nick) = text.strip_prefix("/nick ") {
            print!("-> changed nick to: {}", nick);
        }
    }

    // Listen for `Ctrl+c` and shutdown the node.
    tokio::signal::ctrl_c().await?;

    // TODO: Send "going offline" message on gossip channel.

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
