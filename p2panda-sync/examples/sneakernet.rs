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
use std::collections::{BTreeMap, HashMap, HashSet};

use p2panda_core::logs::{LogHeights, LogRanges, compare};
use p2panda_core::test_utils::TestLog;
use p2panda_core::{AnyOperation, Hash, Operation, SeqNum, SigningKey, Topic, VerifyingKey};
use p2panda_store::logs::LogStore;
use p2panda_store::operations::OperationStore;
use p2panda_store::topics::TopicStore;
use p2panda_store::{SqliteError, SqliteStore, tx};
use p2panda_sync::protocols::ShortFormat;
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;

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

    tx!(store_a, {
        for _ in 0..5 {
            let topic = Topic::random();
            topics_a.insert(topic);

            let log_id = LogId::digest(topic.as_bytes());
            let log_a = TestLog::from_signing_key(signing_key_a.clone());

            for op_i in 0..5 {
                let body = (op_i as usize).to_be_bytes();
                let operation = log_a.operation(&body, CustomExtensions { log_id });

                <SqliteStore as OperationStore<Operation<CustomExtensions>, Hash>>::insert_operation(
                    &store_a,
                    &operation.hash,
                    &operation,
                    &log_id,
                )
                .await?;
            }

            <SqliteStore as TopicStore<Topic, VerifyingKey, LogId>>::associate(
                &store_a, &topic, &node_id_a, &log_id,
            )
            .await?;
        }
    });

    // Export data.

    let mut announcements: Vec<Announcement> = Vec::new();
    let mut operations: HashMap<Topic, Logs> = HashMap::new();

    for topic in &topics_a {
        let all_log_heights = get_topic_log_heights(&store_a, &topic).await?;

        // TODO: This is ugly.
        for (author, log_heights) in &all_log_heights {
            for log_id in log_heights.keys() {
                let log_operations = <SqliteStore as LogStore<
                    Operation<CustomExtensions>,
                    _,
                    _,
                    _,
                    _,
                >>::get_log_entries(
                    &store_a, &node_id_a, log_id, None, None
                )
                .await?
                .unwrap_or_default()
                .into_iter()
                .map(|(operation, _)| AnyOperation {
                    hash: operation.hash,
                    // TODO: This is maybe wrong in p2panda-core: We should be able to convert from
                    // Header<E> to AnyHeader without any errors (the extensions are already in CBOR
                    // AST representation).
                    header: operation.header.try_into().unwrap(),
                    body: operation.body,
                })
                .collect::<Vec<AnyOperation>>();

                let entry = operations.entry(*topic);
                let logs = entry.or_default();

                // This overwrites what was there before but that's not a problem if we've ingested
                // everything from the USB stick before writing.
                logs.insert((*author, *log_id), log_operations);
            }
        }

        // NOTE: Should sign this announcement.
        let announcement = Announcement {
            topic: *topic,
            node_id: node_id_a,
            log_heights: all_log_heights,
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

    tx!(store_b, {
        let log_id = LogId::digest(topic_only_b.as_bytes());
        let log_b = TestLog::from_signing_key(signing_key_b.clone());

        for op_i in 0..5 {
            let body = (op_i as usize).to_be_bytes();
            let operation = log_b.operation(&body, CustomExtensions { log_id });

            <SqliteStore as OperationStore<Operation<CustomExtensions>, Hash>>::insert_operation(
                &store_b,
                &operation.hash,
                &operation,
                &log_id,
            )
            .await?;
        }

        <SqliteStore as TopicStore<Topic, VerifyingKey, LogId>>::associate(
            &store_b,
            &topic_only_b,
            &node_id_b,
            &log_id,
        )
        .await?;
    });

    // B shares one topic with A.
    let topic_a_and_b = topics_a.iter().next().unwrap().clone();
    topics_b.insert(topic_a_and_b);

    tx!(store_b, {
        let log_id = LogId::digest(topic_a_and_b.as_bytes());
        let log_b = TestLog::from_signing_key(signing_key_b.clone());

        for op_i in 0..2 {
            let body = (op_i as usize).to_be_bytes();
            let operation = log_b.operation(&body, CustomExtensions { log_id });

            <SqliteStore as OperationStore<Operation<CustomExtensions>, Hash>>::insert_operation(
                &store_b,
                &operation.hash,
                &operation,
                &log_id,
            )
            .await?;
        }

        <SqliteStore as TopicStore<Topic, VerifyingKey, LogId>>::associate(
            &store_b,
            &topic_a_and_b,
            &node_id_b,
            &log_id,
        )
        .await?;
    });

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
            let log_ranges: LogRanges<VerifyingKey, LogId> =
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
            //
            // TODO: Operations will eventually need validation; in the future we might forward them
            // and ingest on a higher layer which also validates (in stream).
            for (node_id, log_heights) in log_ranges {
                for (log_id, (after, _until)) in log_heights {
                    let after = after.unwrap_or_default() as usize;
                    let operations_we_need = &operations.get(&(node_id, log_id)).unwrap()[after..];

                    for operation in operations_we_need {
                        tx!(store_b, {
                            let operation: Operation<CustomExtensions> =
                                operation.clone().try_into().unwrap();

                            <SqliteStore as OperationStore<Operation<CustomExtensions>, Hash>>::insert_operation(
                                    &store_b,
                                    &operation.hash,
                                    &operation,
                                    &log_id,
                                )
                                .await?;

                            <SqliteStore as TopicStore<Topic, VerifyingKey, LogId>>::associate(
                                &store_b, &topic, &node_id_b, &log_id,
                            )
                            .await?;
                        });
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

            let log_ranges: LogRanges<VerifyingKey, LogId> =
                compare(&our_log_heights, &their_log_heights);

            for (author, log_heights) in log_ranges {
                for (log_id, (after, until)) in log_heights {
                    let log_operations = <SqliteStore as LogStore<
                        Operation<CustomExtensions>,
                        _,
                        _,
                        _,
                        _,
                    >>::get_log_entries(
                        &store_b, &node_id_b, &log_id, after, until
                    )
                    .await?
                    .unwrap_or_default()
                    .into_iter()
                    .map(|(operation, _)| AnyOperation {
                        hash: operation.hash,
                        header: operation.header.try_into().unwrap(),
                        body: operation.body,
                    })
                    .collect::<Vec<AnyOperation>>();

                    // NOTE: We could have a flag here to control if we want to already publish
                    // operations of topics nobody published an announcement for.

                    let entry = operations.entry(topic);
                    let logs = entry.or_default();

                    for operation in log_operations {
                        let log_entry = logs.entry((author, log_id));
                        let log = log_entry.or_default();

                        // NOTE: Appending only the "latest" operations to the log allows us to
                        // build some ring-buffer logic here where we would drop old operations when
                        // running full.
                        log.push(operation);
                    }
                }
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
