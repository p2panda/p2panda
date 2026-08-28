// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sync over "broadcast" transports with mesh-network topologies.
//!
//! This example shows how one could integrate the p2panda sync protocol into any more
//! broadcast-like transport or mesh-network topology, for example on top of LoRa or BLE
//! Advertisements.
//!
//! ## Mesh-Network
//!
//! The approach taken here is a simple "flooding" mesh protocol (some people might call this a
//! routing technique) where each node "repeats" any received message. With the help of checking and
//! storing every message's hash digest in a ring buffer when repeating we make sure to avoid loops.
//!
//! ## Possible improvements
//!
//! 1. This example can easily be extended with a more robust store and forward approach where every
//!    node keeps a cache of the last n operations around, independent of if they are interested in
//!    the content or not. This can be expressed as a independent layer and combined with encryption
//!    (fully encrypted p2panda operations using the topic as symmetric key or p2panda-spaces). We
//!    can imagine a store and forward solution where we don't even know the concrete p2p data-type,
//!    like a simple set, using a bloom-filter to sync the cache with neighbors.
//! 2. Nodes could also eagerly always broadcast new operations without waiting first to sync. Sync
//!    would only be announced / requested when a node is "lagging" behind too much. An out-of-order
//!    buffer will be necessary then to handle operations arriving potentially "out of sync".
//! 3. Usually nodes would like to announce their current states frequently, based on some sort of
//!    interval.
//! 4. Announcement messages should be signed.
//! 5. Message framing might be required to allow sending larger messages for transports with
//!    limited packet sizes.
use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;

use p2panda_core::logs::{LogHeights, LogRanges, compare};
use p2panda_core::traits::{Digest, Provenance};
use p2panda_core::{
    AnyOperation, Body, Hash, Header, Operation, SeqNum, SigningKey, Topic, VerifyingKey,
};
use p2panda_store::logs::LogStore;
use p2panda_store::operations::OperationStore;
use p2panda_store::topics::TopicStore;
use p2panda_store::{SqliteError, SqliteStore, tx};
use p2panda_sync::dedup::DeduplicationBuffer;
use p2panda_sync::protocols::ShortFormat;
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, RwLock, mpsc};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let topic = Topic::random();

    // Spawn three nodes in a broadcast mesh-network. They are directly reachable as such:
    //
    // ```
    // Panda <--> Racoon <--> Icebear
    // ```
    let panda = Node::new().await;
    let racoon = Node::new().await;
    let icebear = Node::new().await;

    println!(
        "Network: [{}] <--> [{}] <--> [{}]",
        panda.id().fmt_short(),
        racoon.id().fmt_short(),
        icebear.id().fmt_short()
    );

    // Publishing new operations will also automatically subscribe to the topic.
    panda.publish(topic, b"Hello!").await?;

    // Expresses interest in a topic and will include it in announcements from now on.
    racoon.subscribe(topic).await;
    icebear.subscribe(topic).await;

    // Finally the nodes are reachable and can receive each other's messages.
    panda.connect(&racoon).await;
    racoon.connect(&icebear).await;

    // Announcements inform everyone in the network about the node's current state (HAVE). Receiving
    // nodes will answer with missing operations if they have something the announcing nodes don't
    // have yet. These messages could be repeated every n seconds.
    panda.announce().await?;
    racoon.announce().await?;
    icebear.announce().await?;

    // Wait a little bit for sync to happen.
    tokio::time::sleep(std::time::Duration::from_millis(250)).await;

    Ok(())
}

#[derive(Clone)]
struct Radio {
    our_tx: mpsc::UnboundedSender<Message>,
    neighbors: Arc<RwLock<Vec<mpsc::UnboundedSender<Message>>>>,
}

impl Radio {
    pub fn new() -> (Self, mpsc::UnboundedReceiver<Message>) {
        let (our_tx, our_rx) = mpsc::unbounded_channel::<Message>();

        (
            Self {
                our_tx,
                neighbors: Arc::new(RwLock::new(Vec::new())),
            },
            our_rx,
        )
    }

    pub async fn add_neighbor(&self, tx: mpsc::UnboundedSender<Message>) {
        let mut neighbors = self.neighbors.write().await;
        neighbors.push(tx);
    }

    pub async fn broadcast(&self, message: Message) {
        let neighbors = self.neighbors.read().await;
        for neighbor in neighbors.iter() {
            let _ = neighbor.send(message.clone());
        }
    }
}

#[derive(Clone)]
struct Mesh {
    dedup: Arc<Mutex<DeduplicationBuffer<Hash>>>,
    radio: Radio,
}

impl Mesh {
    pub fn new(radio: Radio) -> Self {
        Self {
            dedup: Arc::default(),
            radio,
        }
    }

    pub async fn has_seen(&self, message: &Message) -> bool {
        let dedup = self.dedup.lock().await;
        dedup.contains(&message.hash())
    }

    pub async fn flood(&self, message: Message) {
        {
            let mut dedup = self.dedup.lock().await;
            dedup.insert(message.hash());
        }

        self.radio.broadcast(message).await;
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Announcement {
    topic: Topic,
    node_id: VerifyingKey,
    log_heights: BTreeMap<VerifyingKey, BTreeMap<LogId, SeqNum>>,
}

impl Digest<Hash> for Announcement {
    fn hash(&self) -> Hash {
        Hash::digest({
            let mut bytes = Vec::new();
            bytes.extend_from_slice(self.topic.as_bytes());
            bytes.extend_from_slice(self.node_id.as_bytes());
            for (author, log_heights) in &self.log_heights {
                for (log_id, seq_num) in log_heights {
                    bytes.extend_from_slice(author.as_bytes());
                    bytes.extend_from_slice(log_id.as_bytes());
                    bytes.extend_from_slice(&seq_num.to_be_bytes());
                }
            }
            bytes
        })
    }
}

#[derive(Clone, Debug)]
enum Message {
    Operation(Topic, AnyOperation),
    Announcement(Announcement),
}

impl std::fmt::Display for Message {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let kind = match self {
            Self::Operation(_, _) => "operation",
            Self::Announcement(_) => "announcement",
        };

        write!(f, "{}", kind)
    }
}

impl Digest<Hash> for Message {
    fn hash(&self) -> Hash {
        match self {
            Self::Operation(_, operation) => operation.hash(),
            Self::Announcement(announcement) => announcement.hash(),
        }
    }
}

type LogId = Hash;

type LogIds = BTreeMap<VerifyingKey, Vec<LogId>>;

#[derive(Clone, Debug, Serialize, Deserialize)]
struct CustomExtensions {
    log_id: LogId,
}

struct Node {
    topics: Arc<RwLock<HashSet<Topic>>>,
    signing_key: SigningKey,
    store: SqliteStore,
    radio: Radio,
    mesh: Mesh,
}

impl Node {
    pub async fn new() -> Self {
        let signing_key = SigningKey::generate();
        let store = SqliteStore::temporary().await;
        let topics = Arc::new(RwLock::new(HashSet::new()));

        let (radio, mut antenna) = Radio::new();
        let mesh = Mesh::new(radio.clone());

        {
            let topics = topics.clone();
            let store = store.clone();
            let node_id = signing_key.verifying_key();
            let mesh = mesh.clone();

            tokio::task::spawn(async move {
                loop {
                    let Some(message) = antenna.recv().await else {
                        break;
                    };

                    if mesh.has_seen(&message).await {
                        continue;
                    }

                    // Only look at messages for topics we are interested in.
                    let my_topics = topics.read().await;
                    let topic = match &message {
                        Message::Operation(topic, _) => topic,
                        Message::Announcement(announcement) => &announcement.topic,
                    };

                    if my_topics.contains(topic) {
                        match &message {
                            Message::Operation(_, operation) => {
                                println!(
                                    "{}: ingest remote {}",
                                    node_id.fmt_short(),
                                    operation.hash().fmt_short()
                                );

                                // TODO: Validation?

                                let _ = ingest_operation(&store, *topic, operation.clone()).await;
                            }
                            Message::Announcement(announcement) => {
                                let Ok(operations) = compute_diff(
                                    &store,
                                    announcement.topic,
                                    &announcement.log_heights,
                                )
                                .await
                                else {
                                    continue;
                                };

                                for operation in operations {
                                    mesh.flood(Message::Operation(*topic, operation)).await;
                                }
                            }
                        }
                    }

                    // Re-broadcast received message. This "floods" the message in the mesh network
                    // and makes sure the message can be received by other nodes beyond direct
                    // neighbors.
                    println!(
                        "{}: repeat {} {}",
                        node_id.fmt_short(),
                        message,
                        message.hash().fmt_short()
                    );

                    mesh.flood(message).await;
                }
            });
        }

        Self {
            topics,
            signing_key,
            store,
            radio,
            mesh,
        }
    }

    pub async fn connect(&self, other_node: &Node) {
        self.radio
            .add_neighbor(other_node.radio.our_tx.clone())
            .await;
        other_node
            .radio
            .add_neighbor(self.radio.our_tx.clone())
            .await;
    }

    pub fn id(&self) -> VerifyingKey {
        self.signing_key.verifying_key()
    }

    pub async fn announce(&self) -> Result<()> {
        let topics = self.topics.read().await;

        for topic in topics.iter() {
            let all_log_heights = get_topic_log_heights(&self.store, &topic).await?;

            let announcement = Announcement {
                topic: *topic,
                node_id: self.id(),
                log_heights: all_log_heights,
            };

            println!(
                "{}: announces {}",
                self.id().fmt_short(),
                announcement.hash().fmt_short()
            );

            self.mesh.flood(Message::Announcement(announcement)).await;
        }

        Ok(())
    }

    pub async fn subscribe(&self, topic: Topic) {
        let mut topics = self.topics.write().await;
        topics.insert(topic);
    }

    pub async fn publish(&self, topic: Topic, body: &[u8]) -> Result<()> {
        self.subscribe(topic).await;

        let operation = create_operation(&self.store, &self.signing_key, topic, body).await?;

        println!(
            "{}: create & ingest operation {}",
            self.id().fmt_short(),
            operation.hash().fmt_short()
        );

        self.mesh.flood(Message::Operation(topic, operation)).await;

        Ok(())
    }
}

// TODO: A lot of methods we probably want to move somewhere else:

async fn create_operation(
    store: &SqliteStore,
    signing_key: &SigningKey,
    topic: Topic,
    body: &[u8],
) -> Result<AnyOperation> {
    let body: Option<Body> = Some(Body::from_bytes(body));

    let log_id = LogId::digest(topic.as_bytes());
    let extensions = CustomExtensions { log_id };

    let operation = tx!(store, {
        let (seq_num, backlink) = <SqliteStore as LogStore<
            Operation,
            VerifyingKey,
            LogId,
            SeqNum,
            Hash,
        >>::get_latest_entry_tx(
            &store, &signing_key.verifying_key(), &log_id
        )
        .await?
        .map(|operation| (operation.header.seq_num + 1, Some(operation.hash)))
        .unwrap_or((0, None));

        let header = {
            let mut builder = Header::builder().seq_num(seq_num).backlink(backlink);

            if let Some(ref body) = body {
                builder = builder.body(body);
            }

            builder.build(&signing_key, extensions)
        };

        <SqliteStore as TopicStore<Topic, VerifyingKey, LogId>>::associate(
            &store,
            &topic,
            &signing_key.verifying_key(),
            &log_id,
        )
        .await?;

        let operation = Operation::from_parts(header, body);

        store
            .insert_operation(&operation.hash, &operation, &log_id)
            .await?;

        operation
    });

    Ok(AnyOperation {
        hash: operation.hash,
        header: operation
            .header
            .try_into()
            .expect("shouldn't be an error in p2panda-core"),
        body: operation.body,
    })
}

async fn ingest_operation(
    store: &SqliteStore,
    topic: Topic,
    operation: AnyOperation,
) -> Result<()> {
    let operation: Operation<CustomExtensions> = operation.try_into()?;

    tx!(store, {
        <SqliteStore as TopicStore<Topic, VerifyingKey, LogId>>::associate(
            &store,
            &topic,
            &operation.author(),
            &operation.header.extensions.log_id,
        )
        .await?;

        store
            .insert_operation(
                &operation.hash,
                &operation,
                &operation.header.extensions.log_id,
            )
            .await?;

        operation
    });

    Ok(())
}

async fn compute_diff(
    store: &SqliteStore,
    topic: Topic,
    their_log_heights: &LogHeights<VerifyingKey, LogId>,
) -> Result<Vec<AnyOperation>> {
    let our_log_heights = get_topic_log_heights(&store, &topic).await?;
    let log_ranges: LogRanges<VerifyingKey, LogId> = compare(&our_log_heights, &their_log_heights);
    let mut operations = Vec::new();

    for (author, log_heights) in log_ranges {
        for (log_id, (after, until)) in log_heights {
            let log_operations = <SqliteStore as LogStore<
                Operation<CustomExtensions>,
                _,
                _,
                _,
                _,
            >>::get_log_entries(
                &store, &author, &log_id, after, until
            )
            .await?
            .unwrap_or_default();

            for (operation, _) in log_operations {
                let operation = AnyOperation {
                    hash: operation.hash,
                    // TODO: This is wrong in p2panda-core and should not be fallible.
                    header: operation.header.try_into().unwrap(),
                    body: operation.body,
                };

                operations.push(operation);
            }
        }
    }

    Ok(operations)
}

async fn get_topic_log_heights(
    store: &SqliteStore,
    topic: &Topic,
) -> std::result::Result<LogHeights<VerifyingKey, LogId>, SqliteError> {
    let logs: LogIds = store.resolve(topic).await?;
    let log_heights = get_log_heights(&store, &logs).await?;

    Ok(log_heights)
}

async fn get_log_heights(
    store: &SqliteStore,
    logs: &LogIds,
) -> std::result::Result<LogHeights<VerifyingKey, LogId>, SqliteError> {
    let mut result = BTreeMap::new();

    for (verifying_key, log_ids) in logs {
        let Some(log_heights) =
            LogStore::<Operation, VerifyingKey, LogId, SeqNum, Hash>::get_log_heights(
                store,
                verifying_key,
                log_ids,
            )
            .await?
        else {
            continue;
        };

        result.insert(*verifying_key, log_heights);
    }

    Ok(result)
}
