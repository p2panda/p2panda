// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sync via USB-sticks (sneakernet).
//!
//! This example shows how one can not only drop p2panda operations onto an USB-stick to deliver
//! them somewhere but also how we can use it to run an _interactive_, **delay-tolerant sync
//! protocol** where every node shares its current state with the network, using the USB-stick as a
//! "very slow" transport.
//!
//! With this approach we efficiently only write the requested ranges to the USB-stick after we've
//! considered every node's "needs" when reading the stick. One could also describe this as sync in
//! a **broadcast** network.
//!
//! The USB-stick functions as a **store and forward** buffer where data is persisted from other
//! participants even if the current node reading or writing to the USB-stick is not interested in
//! it. The USB-stick helps to eventually dissiminate data to everyone in the network.
//!
//! Note that sync via USB-stick is possible to do in both encrypted & unencrypted scenarios. For
//! fully encrypted data on the stick, we would attempt decrypting all ciphertexts first or leave a
//! "hint" for which ciphertexts are likely interesting for us by coming up with a file name or path
//! scheme.
//!
//! ## Protocol
//!
//! 1. Check what's inside the USB stick, compute diff, load delta of everything what we are
//!    interested in and don't have yet.
//!
//!    We don't necessarily need to compute a diff, could also just forward whatever there is.
//!    Applications would see potentially more duplicates then. Having an idempotent processing
//!    logic is key (probably should have that in any case).
//!
//!    Steps:
//!
//!    - We need awareness of topics we are interested in
//!    - Resolve log ids + authors for all topics
//!    - Get log heights for all log ids
//!    - Compute diff between local log heights and the operations from USB stick
//!
//! 2. Check announcements of other nodes (if there's any).
//!
//!    Steps (per announcement):
//!
//!    - Resolve log ids + authors for all topics
//!    - Get log heights for all log ids
//!    - Compute diff between local log heights and the ones from the announcements on USB stick
//!    - Get operations from diff from our local store and write to file on USB
//!
//! 3. Compute our own state vector & write it as announcement message to USB stick.
//!
//!    We want to **ingest** things _before_ computing our state vector / announcement, maybe
//!    there's already stuff I've needed nonetheless and we don't want to announce needing data we
//!    could already have.
//!
//! ## Possible improvements
//!
//! This can easily be extended to something ready-to-use, for example:
//!
//! 1. Encrypting all announcements and operations by using the topic as symmetric secret key. Later
//!    we can even support revocation with using the group state of `p2panda-spaces`.
//! 2. Introducing a ring buffer logic with a configured max. USB-stick capacity. For example we can
//!    define that p2panda can only occupy max. 5GB on the stick. Old operations will be deleted
//!    when new one's come in (first-in, first-out).
//! 3. Nice command-line-interface to run the sync protocol using any p2panda SQLite database.
//!
//! ## How do I know what my node "is interested in"?
//!
//! Most of our APIs express this by having an active "topic handle" where the topic is known. The
//! topic itself usually came via a side-channel and is treated as a secret.
//!
//! You can manage a similar list yourself where you populate the list with the known topics this
//! node currently wants to actively sync over.
//!
//! We probably also want to offer an API where we can query all known topics from the database,
//! however this implies that you will sync over _everything_ you ever announced interested in,
//! which is sometimes not desirable.
mod common;

use std::collections::{BTreeMap, HashMap, HashSet};

use futures_util::StreamExt;
use p2panda_core::logs::{LogHeights, LogRanges, compare};
use p2panda_core::traits::Provenance;
use p2panda_core::{AnyOperation, Hash, Operation, SeqNum, SigningKey, Topic, VerifyingKey};
use p2panda_store::logs::LogStore;
use p2panda_store::topics::TopicStore;
use p2panda_store::{SqliteError, SqliteStore};
use p2panda_sync::api::{StreamItem, ingest_operation, log_ranges};
use p2panda_sync::protocols::ShortFormat;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

use crate::common::create_operation;

#[derive(Debug, PartialEq, Eq)]
struct Announcement {
    topic: Topic,
    node_id: VerifyingKey,
    log_heights: BTreeMap<VerifyingKey, BTreeMap<LogId, SeqNum>>,
}

impl std::hash::Hash for Announcement {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.topic.hash(state);
        self.node_id.hash(state);
    }
}

#[derive(Debug)]
struct UsbStick(Mutex<UsbStickContent>);

impl UsbStick {
    pub fn new() -> Self {
        Self(Mutex::new(UsbStickContent {
            announcements: HashSet::new(),
            operations: HashMap::new(),
        }))
    }
}

type LogId = Hash;

// TODO: Find a place.
pub type LogIds = BTreeMap<VerifyingKey, Vec<LogId>>;

type Logs = HashMap<(VerifyingKey, LogId), Vec<AnyOperation>>;

#[derive(Debug)]
struct UsbStickContent {
    announcements: HashSet<Announcement>,
    // NOTE: Could be ring-buffer (we reserve capacity in a config), or support-node-like k/v store.
    // In both cases this would allow us to _not_ mention the topic & work with encryption.
    operations: HashMap<Topic, Logs>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct CustomExtensions {
    log_id: LogId,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Node A.

    // Populate data.

    let signing_key_a = SigningKey::generate();
    let node_id_a = signing_key_a.verifying_key();
    let store_a = SqliteStore::temporary().await;

    // TODO: We want a method on TopicStore to give us _all_ topics.
    let mut topics_a = HashSet::<Topic>::new();

    for _ in 0..5 {
        let topic = Topic::random();
        topics_a.insert(topic);

        for op_i in 0..5 {
            let body = (op_i as usize).to_be_bytes();
            create_operation(&store_a, &signing_key_a, topic, &body).await?;
        }
    }

    // Export data.

    let mut announcements: Vec<Announcement> = Vec::new();
    let mut operations: HashMap<Topic, Logs> = HashMap::new();

    for topic in &topics_a {
        let local_log_heights = get_topic_log_heights(&store_a, &topic).await?;
        let remote_log_heights = LogHeights::default();
        let diff = compare(&local_log_heights, &remote_log_heights);
        let mut operation_stream = log_ranges(&store_a, diff);

        if let Some(result) = operation_stream.next().await {
            let StreamItem {
                entry: operation,
                log_id,
                ..
            } = result?;
            let logs = operations.entry(*topic).or_default();
            logs.entry((operation.author(), log_id))
                .or_default()
                .push(operation);
        }

        // NOTE: Should sign this announcement.
        let announcement = Announcement {
            topic: *topic,
            node_id: node_id_a,
            log_heights: local_log_heights,
        };

        announcements.push(announcement);
    }

    // Write to USB.

    let stick = UsbStick::new();

    // NOTE: Could encrypt all data using the topic as a symmetric key.
    {
        let mut stick = stick.0.lock().await;

        for announcement in announcements {
            stick.announcements.insert(announcement);
        }

        for (topic, our_logs) in operations {
            let stick_entry = stick.operations.entry(topic);
            let stick_logs = stick_entry.or_default();

            for ((author, log_id), operations) in our_logs.into_iter() {
                stick_logs.insert((author, log_id), operations);
            }
        }
    }

    // Handover is complete. Node B has the stick.

    let signing_key_b = SigningKey::generate();
    let node_id_b = signing_key_b.verifying_key();
    let store_b = SqliteStore::temporary().await;

    let mut topics_b = HashSet::<Topic>::new();

    // B creates some data.

    let topic_only_b = Topic::random();
    topics_b.insert(topic_only_b);

    for op_i in 0..5 {
        let body = (op_i as usize).to_be_bytes();
        create_operation(&store_b, &signing_key_b, topic_only_b, &body).await?;
    }

    // B shares one topic with A.
    let topic_a_and_b = topics_a.iter().next().unwrap().clone();
    topics_b.insert(topic_a_and_b);

    for op_i in 0..2 {
        let body = (op_i as usize).to_be_bytes();
        create_operation(&store_b, &signing_key_b, topic_a_and_b, &body).await?;
    }

    // Import data.

    {
        let stick = stick.0.lock().await;

        for announcement in &stick.announcements {
            // Ignore our own previous announcements.
            if announcement.node_id == node_id_b {
                continue;
            }

            // Ignore topics we are not interested in.
            if !topics_b.contains(&announcement.topic) {
                continue;
            }

            let topic = announcement.topic;
            let their_log_heights = &announcement.log_heights;
            let our_log_heights = {
                let logs: LogIds = store_b.resolve(&topic).await?;
                get_log_heights(&store_b, &logs).await?
            };

            // Determine the operations we need from the stick.
            let diff: LogRanges<VerifyingKey, LogId> =
                // NOTE: we reverse the roles here, "their" and "our", because we are determining
                // what they should "send" to us...not what we should send to them.
                //
                // The docs for `compare()` could maybe be updated to reflect this bidirectional
                // nature.
                compare(&their_log_heights, &our_log_heights);

            // Get all stick operations for the announcement topic.
            let mut operations = HashMap::new();
            if let Some(ops) = &stick.operations.get(&topic) {
                operations.extend(*ops)
            }

            // Insert all desired stick operations into our store.
            for (node_id, log_heights) in diff {
                for (log_id, (after, _until)) in log_heights {
                    let after = after.unwrap_or_default() as usize;
                    let operations_we_need = &operations.get(&(node_id, log_id)).unwrap()[after..];

                    for operation in operations_we_need {
                        // TODO: Clone can be removed after OooBuffer PR was merged.
                        let operation: Operation<CustomExtensions> =
                            operation.clone().try_into()?;

                        ingest_operation(&store_b, None, &operation, &log_id, &topic, false)
                            .await?;
                    }
                }
            }
        }
    }

    {
        // Export data.

        let mut stick = stick.0.lock().await;

        let mut announcements: Vec<Announcement> = Vec::new();
        let mut operations: HashMap<Topic, Logs> = HashMap::new();

        for topic in topics_b {
            let our_log_heights = get_topic_log_heights(&store_b, &topic).await?;

            // Write out diff of operations others don't have yet.

            let their_log_heights = {
                stick
                    .announcements
                    .iter()
                    .find(|announcement| announcement.topic == topic)
                    .map(|announcement| announcement.log_heights.clone())
                    .unwrap_or_default()
            };

            let diff = compare(&our_log_heights, &their_log_heights);
            let mut operation_stream = log_ranges(&store_a, diff);

            if let Some(result) = operation_stream.next().await {
                let StreamItem {
                    entry: operation,
                    log_id,
                    ..
                } = result?;
                let logs = operations.entry(topic).or_default();

                // NOTE: Appending only the "latest" operations to the log allows us to
                // build some ring-buffer logic here where we would drop old operations when
                // running full.
                logs.entry((operation.author(), log_id))
                    .or_default()
                    .push(operation);
            }

            // Write out our own state.

            let announcement = Announcement {
                topic,
                node_id: node_id_b,
                log_heights: our_log_heights,
            };

            announcements.push(announcement);
        }

        // Write to USB.

        for announcement in announcements {
            stick.announcements.insert(announcement);
        }

        for (topic, our_logs) in operations {
            let stick_entry = stick.operations.entry(topic);
            let stick_logs = stick_entry.or_default();

            for ((author, log_id), operations) in our_logs.into_iter() {
                stick_logs.insert((author, log_id), operations);
            }
        }
    }

    {
        let stick = stick.0.lock().await;

        println!("node_a: {}", node_id_a.fmt_short());
        println!("node_b: {}", node_id_b.fmt_short());

        println!("\nANNOUNCEMENTS:\n");

        for announcement in &stick.announcements {
            println!("node_id: {}", announcement.node_id.fmt_short());
            println!("topic: {}", announcement.topic.to_hex()[0..6].to_string());
            println!("log heights:");

            for (author, log_heights) in &announcement.log_heights {
                for (log_id, log_height) in log_heights {
                    println!(
                        "* {}/{} log_height={}",
                        author.fmt_short(),
                        log_id.fmt_short(),
                        log_height
                    );
                }
            }

            println!("---");
        }

        println!("\nOPERATIONS:\n");

        for (topic, logs) in &stick.operations {
            println!("topic: {}", topic.to_hex()[0..6].to_string());

            for ((author, log_id), operations) in logs {
                println!(
                    "* {}/{} log_height={}",
                    author.fmt_short(),
                    log_id.fmt_short(),
                    operations.len() - 1
                );
            }

            println!("---");
        }
    }

    Ok(())
}

// TODO: Find a place.
async fn get_topic_log_heights(
    store: &SqliteStore,
    topic: &Topic,
) -> Result<LogHeights<VerifyingKey, LogId>, SqliteError> {
    let logs: LogIds = store.resolve(topic).await?;
    let log_heights = get_log_heights(&store, &logs).await?;

    Ok(log_heights)
}

// TODO: Find a place.
async fn get_log_heights(
    store: &SqliteStore,
    logs: &LogIds,
) -> Result<LogHeights<VerifyingKey, LogId>, SqliteError> {
    let mut result = BTreeMap::new();

    for (verifying_key, log_ids) in logs {
        let Some(log_heights) = store.get_log_heights(verifying_key, log_ids).await? else {
            continue;
        };

        result.insert(*verifying_key, log_heights);
    }

    Ok(result)
}
