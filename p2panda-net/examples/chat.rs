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
use p2panda_core::cbor::{decode_cbor, encode_cbor};
use p2panda_core::{Body, Hash, Header, Operation, PrivateKey, PublicKey};
use p2panda_net::addrs::NodeInfo;
use p2panda_net::iroh_endpoint::from_public_key;
use p2panda_net::iroh_mdns::MdnsDiscoveryMode;
use p2panda_net::utils::ShortFormat;
use p2panda_net::{
    AddressBook, Discovery, Endpoint, Gossip, LogSync, MdnsDiscovery, NodeId, TopicId,
};
use p2panda_store::{MemoryStore, OperationStore};
use p2panda_sync::traits::TopicLogMap;
use p2panda_sync::{Logs, TopicLogSyncEvent};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha12Rng;
use serde::{Deserialize, Serialize};
use tokio::sync::{RwLock, mpsc};
use tokio::time::Instant;
use tracing::info;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::prelude::*;

type LogId = u64;

const NETWORK_ID: [u8; 32] = [7; 32];

const RELAY_URL: &str = "https://euc1-1.relay.n0.iroh-canary.iroh.link.";

/// This application maintains only one log per author, this is why we can hard-code it.
const LOG_ID: LogId = 1;

/// Heartbeat message to be sent over gossip (ephemeral messaging).
#[derive(Debug, Serialize, Deserialize)]
struct Heartbeat {
    sender: PublicKey,
    online: bool,
    rnd: u64,
}

impl Heartbeat {
    fn new(sender: PublicKey, online: bool) -> Self {
        Self {
            sender,
            online,
            rnd: rand::random(),
        }
    }
}

pub fn setup_logging() {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
        .with(EnvFilter::from_default_env())
        .try_init()
        .ok();
}

#[derive(Parser)]
struct Args {
    /// Bootstrap node identifier.
    #[arg(short = 'b', long, value_name = "BOOTSTRAP_ID")]
    bootstrap_id: Option<NodeId>,

    /// Chat topic identifier.
    #[arg(short = 'c', long, value_name = "CHAT_TOPIC_ID")]
    chat_topic_id: Option<String>,

    /// Enable mDNS discovery
    #[arg(short = 'm', long, action)]
    mdns: bool,
}

#[derive(Clone, Default, Debug)]
pub struct ChatTopicMap(Arc<RwLock<HashMap<TopicId, Logs<LogId>>>>);

impl ChatTopicMap {
    async fn insert(&self, topic_id: TopicId, node_id: NodeId, log_id: LogId) {
        let mut map = self.0.write().await;
        map.entry(topic_id)
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

    let private_key = PrivateKey::new();
    let public_key = private_key.public_key();

    // Retrieve the chat topic ID from the provided arguments, otherwise generate a new, random,
    // cryptographically-secure identifier.
    let topic_id: TopicId = if let Some(topic) = args.chat_topic_id {
        hex::decode(topic)
            .expect("topic id should be valid hex")
            .try_into()
            .expect("topic id should be 32 bytes")
    } else {
        let mut rng = ChaCha12Rng::from_os_rng();
        rng.random()
    };

    println!("network id: {}", NETWORK_ID.fmt_short());
    println!("chat topic id: {}", hex::encode(topic_id));
    println!("public key: {}", public_key.to_hex());
    println!("relay url: {}", RELAY_URL);

    // Set up sync for p2panda operations.
    let mut store = ChatStore::new();

    let topic_map = ChatTopicMap::default();
    topic_map.insert(topic_id, public_key, LOG_ID).await;

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
        .spawn()
        .await
        .unwrap();

    let mdns_discovery_mode = if args.mdns {
        MdnsDiscoveryMode::Active
    } else {
        MdnsDiscoveryMode::Passive
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

    // Subscribe to gossip overlay to receive and publish (ephemeral) messages.
    let heartbeat_tx = gossip.stream(topic_id).await.unwrap();
    let mut heartbeat_rx = heartbeat_tx.subscribe();

    let final_heartbeat_tx = gossip.stream(topic_id).await.unwrap();

    // Mapping of public key to nickname.
    let nicknames = Arc::new(RwLock::new(HashMap::<PublicKey, String>::new()));

    // Mapping of public key to the instant that the last heartbeat message was received.
    let status = Arc::new(RwLock::new(HashMap::new()));

    // Publish (ephemeral) heartbeat messages.
    tokio::task::spawn(async move {
        loop {
            // Create and serialize a heartbeat message.
            let msg = Heartbeat::new(public_key, true);
            let encoded_msg = encode_cbor(&msg).unwrap();

            // Publish the message to the gossip topic.
            heartbeat_tx.publish(encoded_msg).await.unwrap();

            tokio::time::sleep(Duration::from_secs(rand::random_range(20..30))).await;
        }
    });

    // Receive and log each (ephemeral) heartbeat message.
    {
        let nicknames = Arc::clone(&nicknames);
        let status = Arc::clone(&status);
        tokio::spawn(async move {
            loop {
                if let Some(Ok(message)) = heartbeat_rx.next().await {
                    let msg: Heartbeat = decode_cbor(&message[..]).expect("valid cbor encoding");

                    info!("received heartbeat: {:?}", msg);

                    // Look up nickname for sender.
                    let name = if let Some(nickname) = nicknames.read().await.get(&msg.sender) {
                        nickname.to_owned()
                    } else {
                        msg.sender.fmt_short()
                    };

                    // Update status hashmap.
                    if status
                        .write()
                        .await
                        .insert(msg.sender, Instant::now())
                        .is_none()
                    {
                        println!("-> {} came online", name)
                    }

                    if !msg.online {
                        status.write().await.remove(&msg.sender);
                        println!("-> {} went offline", name)
                    }
                }
            }
        });
    }

    let sync = LogSync::builder(store.clone(), topic_map.clone(), endpoint, gossip)
        .spawn()
        .await
        .unwrap();

    let sync_tx = sync.stream(topic_id, true).await.unwrap();
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

                        topic_map
                            .insert(topic_id, operation.header.public_key, LOG_ID)
                            .await;
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
    tokio::task::spawn(async move {
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
    });

    // Listen for `Ctrl+c` and shutdown the node.
    tokio::signal::ctrl_c().await.unwrap();

    // Create and serialize a final heartbeat message.
    //
    // This informs other chatters that we are going offline.
    let msg = Heartbeat::new(public_key, false);
    let encoded_msg = encode_cbor(&msg).unwrap();

    final_heartbeat_tx.publish(&encoded_msg[..]).await.unwrap();

    // Sleep briefly to allow sending of heartbeat message.
    tokio::time::sleep(Duration::from_millis(100)).await;

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
