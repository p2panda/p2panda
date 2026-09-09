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
use p2panda_core::traits::Provenance;
use p2panda_core::{AnyOperation, Hash, Operation, SeqNum, SigningKey, Topic, VerifyingKey};
use p2panda_store::topics::TopicStore;
use p2panda_store::{SqliteError, SqliteStore, Transaction};
use p2panda_sync::api::{
    LogHeights, compare_logs, ingest_operation, log_heights, log_ranges, topic_log_heights,
};
use p2panda_sync::protocols::ShortFormat;
use tokio::sync::Mutex;

use crate::common::{CustomExtensions, LogId, create_operation};

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

type Logs = HashMap<(VerifyingKey, LogId), Vec<AnyOperation>>;

#[derive(Debug)]
struct UsbStickContent {
    announcements: HashSet<Announcement>,
    // NOTE: Could be ring-buffer (we reserve capacity in a config), or support-node-like k/v store.
    // In both cases this would allow us to _not_ mention the topic & work with encryption.
    operations: HashMap<Topic, Logs>,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Panda.

    // Populate data.

    let panda_signing_key = SigningKey::generate();
    let panda_id = panda_signing_key.verifying_key();
    let panda_store = SqliteStore::temporary().await;

    for _ in 0..5 {
        let topic = Topic::random();
        subscribe(&panda_store, &panda_signing_key, topic).await?;

        for op_i in 0..5 {
            let body = (op_i as usize).to_be_bytes();
            create_operation(&panda_store, &panda_signing_key, topic, &body).await?;
        }
    }

    // Export data.

    let mut announcements: Vec<Announcement> = Vec::new();
    let mut operations: HashMap<Topic, Logs> = HashMap::new();

    let permit = panda_store.begin().await?;
    let panda_topics =
        <SqliteStore as TopicStore<Topic, VerifyingKey, LogId>>::topics(&panda_store).await?;
    panda_store.commit(permit).await?;

    // For every topic we want to export our announcement and all logs onto the USB stick.
    for topic in &panda_topics {
        let panda_log_heights = topic_log_heights(&panda_store, topic).await?;
        // Use default (empty) log heights for remote as we want to export everything.
        let mut operation_stream = log_ranges(
            &panda_store,
            compare_logs(&panda_log_heights, &LogHeights::default()),
        );

        if let Some(Ok(log)) = operation_stream.next().await {
            let operation = log.entry;
            let logs = operations.entry(*topic).or_default();
            logs.entry((operation.author(), log.log_id))
                .or_default()
                .push(operation);
        }

        // NOTE: Should sign this announcement.
        let panda_announcement = Announcement {
            topic: *topic,
            node_id: panda_id,
            log_heights: panda_log_heights,
        };

        announcements.push(panda_announcement);
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

    // Handover is complete. Sloth has the stick.

    let sloth_signing_key = SigningKey::generate();
    let sloth_id = sloth_signing_key.verifying_key();
    let sloth_store = SqliteStore::temporary().await;

    // Sloth creates some data.

    let sloth_topic_only = Topic::random();
    subscribe(&sloth_store, &sloth_signing_key, sloth_topic_only).await?;

    for op_i in 0..5 {
        let body = (op_i as usize).to_be_bytes();
        create_operation(&sloth_store, &sloth_signing_key, sloth_topic_only, &body).await?;
    }

    // Ensure that Sloth shares one topic with Panda.
    let sloth_and_panda_topics = panda_topics.iter().next().unwrap().clone();
    subscribe(&sloth_store, &sloth_signing_key, sloth_and_panda_topics).await?;

    for op_i in 0..2 {
        let body = (op_i as usize).to_be_bytes();
        create_operation(
            &sloth_store,
            &sloth_signing_key,
            sloth_and_panda_topics,
            &body,
        )
        .await?;
    }

    // Import data.
    let permit = sloth_store.begin().await?;
    let sloth_topics =
        <SqliteStore as TopicStore<Topic, VerifyingKey, LogId>>::topics(&sloth_store).await?;
    sloth_store.commit(permit).await?;

    {
        let stick = stick.0.lock().await;

        for announcement in &stick.announcements {
            // Ignore our own previous announcements.
            if announcement.node_id == sloth_id {
                continue;
            }

            // Ignore topics we are not interested in.
            if !sloth_topics.contains(&announcement.topic) {
                continue;
            }

            let topic = announcement.topic;
            let panda_log_heights = &announcement.log_heights;
            let sloth_log_heights = {
                let logs = sloth_store.resolve(&topic).await?;
                log_heights(&sloth_store, &logs).await?
            };

            // Determine the operations we need from the stick.
            let diff =
                // NOTE: we reverse the roles here, "their" and "our", because we are determining
                // what they should "send" to us...not what we should send to them.
                //
                // The docs for `compare_logs()` could maybe be updated to reflect this bidirectional
                // nature.
                compare_logs(&panda_log_heights, &sloth_log_heights);

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

                        ingest_operation(&sloth_store, None, &operation, &log_id, &topic, false)
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

        for topic in sloth_topics {
            let sloth_log_heights = topic_log_heights(&sloth_store, &topic).await?;

            // Write out diff of operations others don't have yet.

            let panda_log_heights = {
                stick
                    .announcements
                    .iter()
                    .find(|announcement| announcement.topic == topic)
                    .map(|announcement| announcement.log_heights.clone())
                    .unwrap_or_default()
            };

            let mut operation_stream = log_ranges(
                &sloth_store,
                compare_logs(&sloth_log_heights, &panda_log_heights),
            );

            if let Some(Ok(log)) = operation_stream.next().await {
                let operation = log.entry;
                let logs = operations.entry(topic).or_default();

                // NOTE: Appending only the "latest" operations to the log allows us to build some
                // ring-buffer logic here where we would drop old operations when running full.
                logs.entry((operation.author(), log.log_id))
                    .or_default()
                    .push(operation);
            }

            // Write out our own state.

            let sloth_announcement = Announcement {
                topic,
                node_id: sloth_id,
                log_heights: sloth_log_heights,
            };

            announcements.push(sloth_announcement);
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

        println!("panda: {}", panda_id.fmt_short());
        println!("sloth: {}", sloth_id.fmt_short());

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

async fn subscribe(
    store: &SqliteStore,
    signing_key: &SigningKey,
    topic: Topic,
) -> Result<(), SqliteError> {
    let permit = store.begin().await?;
    store
        .associate(
            &topic,
            &signing_key.verifying_key(),
            &Hash::digest(topic.as_bytes()),
        )
        .await?;
    store.commit(permit).await?;

    Ok(())
}
