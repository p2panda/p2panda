// SPDX-License-Identifier: MIT OR Apache-2.0

//! Session-based sync example (w. eager push as "live-mode").
//!
//! Node A sends an announcement.
//! Node B responds with operations and their own announcement.
//! Node A ingests the operations and responds with operations.
//! Node B creates a new operation and sends it immediately.
//!
//! To keep the example simple, messages (in the forms of announcements and operations) are passed
//! directly between nodes. In a real-world context the messages would be sent over an underlying
//! connection of some kind (e.g. iroh's QUIC streams).
use std::collections::BTreeMap;

use p2panda_core::logs::{LogHeights, LogRanges, compare};
use p2panda_core::test_utils::TestLog;
use p2panda_core::traits::Digest;
use p2panda_core::{AnyOperation, Hash, Operation, SeqNum, SigningKey, Topic, VerifyingKey};
use p2panda_store::logs::LogStore;
use p2panda_store::operations::OperationStore;
use p2panda_store::topics::TopicStore;
use p2panda_store::{SqliteStore, tx};
use p2panda_sync::protocols::ShortFormat;
use serde::{Deserialize, Serialize};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

type LogId = Hash;
type LogIds = BTreeMap<VerifyingKey, Vec<LogId>>;

#[derive(Clone, Debug, Serialize, Deserialize)]
struct CustomExtensions {
    log_id: LogId,
}

// We assume the topic is known as a result of the sync session context.
#[derive(Debug, PartialEq, Eq)]
struct Announcement {
    log_heights: BTreeMap<VerifyingKey, BTreeMap<LogId, SeqNum>>,
}

impl Digest<Hash> for Announcement {
    fn hash(&self) -> Hash {
        Hash::digest({
            let mut bytes = Vec::new();

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

#[derive(Debug)]
struct Node {
    signing_key: SigningKey,
    store: SqliteStore,
}

impl Node {
    async fn new() -> Self {
        Self {
            signing_key: SigningKey::generate(),
            store: SqliteStore::temporary().await,
        }
    }

    fn id(&self) -> VerifyingKey {
        self.signing_key.verifying_key()
    }

    /// Create a new log, populate it with five operations and associate it with the topic.
    async fn populate_log(&self, topic: Topic) -> Result<TestLog> {
        let log = TestLog::from_signing_key(self.signing_key.clone());

        tx!(self.store, {
            let log_id = LogId::digest(topic.as_bytes());

            for op_i in 0..5 {
                let body = (op_i as usize).to_be_bytes();
                let operation = log.operation(&body, CustomExtensions { log_id });

                <SqliteStore as OperationStore<Operation<CustomExtensions>, Hash>>::insert_operation(
                    &self.store,
                    &operation.hash,
                    &operation,
                    &log_id,
                )
                .await?;
            }

            <SqliteStore as TopicStore<Topic, VerifyingKey, LogId>>::associate(
                &self.store,
                &topic,
                &self.id(),
                &log_id,
            )
            .await?;
        });

        Ok(log)
    }

    /// Query the local log heights for the topic and return an announcement.
    async fn generate_announcement(&self, topic: Topic) -> Result<Announcement> {
        let log_heights = get_topic_log_heights(&self.store, &topic).await?;

        Ok(Announcement { log_heights })
    }

    /// Process an announcement received from a remote node.
    ///
    /// Query the local log heights for the topic, compare them with the remote log heights,
    /// retrieve any operations needed by the remote and return them to be transmitted.
    async fn process_announcement(
        &self,
        announcement: Announcement,
        topic: Topic,
    ) -> Result<Vec<AnyOperation>> {
        let their_log_heights = &announcement.log_heights;
        let our_log_heights = get_topic_log_heights(&self.store, &topic).await?;

        let log_ranges: LogRanges<VerifyingKey, LogId> =
            compare(&our_log_heights, &their_log_heights);

        let mut operations = Vec::new();

        for (_author, log_heights) in log_ranges {
            for (log_id, (after, until)) in log_heights {
                let log_operations = <SqliteStore as LogStore<
                    Operation<CustomExtensions>,
                    _,
                    _,
                    _,
                    _,
                >>::get_log_entries(
                    &self.store, &self.id(), &log_id, after, until
                )
                .await?
                .unwrap_or_default()
                .into_iter()
                .map(|(operation, _)| AnyOperation {
                    hash: operation.hash,
                    header: operation
                        .header
                        .try_into()
                        .expect("shouldn't be an error in p2panda-core"),
                    body: operation.body,
                })
                .collect::<Vec<AnyOperation>>();

                operations.extend(log_operations);
            }
        }

        Ok(operations)
    }

    /// Insert an operation into the store and associate the log with the given topic.
    async fn ingest_operation(&self, topic: Topic, operation: AnyOperation) -> Result<()> {
        let log_id = LogId::digest(topic.as_bytes());

        tx!(self.store, {
            let operation: Operation<CustomExtensions> = operation.clone().try_into()?;

            <SqliteStore as OperationStore<Operation<CustomExtensions>, Hash>>::insert_operation(
                &self.store,
                &operation.hash,
                &operation,
                &log_id,
            )
            .await?;

            <SqliteStore as TopicStore<Topic, VerifyingKey, LogId>>::associate(
                &self.store,
                &topic,
                &self.id(),
                &log_id,
            )
            .await?;
        });

        Ok(())
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let topic = Topic::random();

    // Node A and B populate their logs with operations.

    let node_a = Node::new().await;
    let _log_a = node_a.populate_log(topic).await?;
    println!("{}: node a", node_a.id().fmt_short());

    let node_b = Node::new().await;
    let log_b = node_b.populate_log(topic).await?;
    println!("{}: node b", node_b.id().fmt_short());

    println!("--------");

    // Node A sends an announcement to node B.

    let announcement_a = node_a.generate_announcement(topic).await?;

    println!(
        "{}: process remote announcement {}",
        node_b.id().fmt_short(),
        announcement_a.hash().fmt_short()
    );

    // Node B processes the announcement then sends operations and an announcement.

    let log_operations_b = node_b.process_announcement(announcement_a, topic).await?;
    let announcement_b = node_b.generate_announcement(topic).await?;

    println!("--------");

    // Node A processes the operations and announcement from node B then sends operations.

    for operation in log_operations_b {
        println!(
            "{}: ingest remote operation {}",
            node_a.id().fmt_short(),
            operation.hash().fmt_short()
        );

        node_a.ingest_operation(topic, operation).await?;
    }

    println!(
        "{}: process remote announcement {}",
        node_a.id().fmt_short(),
        announcement_b.hash().fmt_short()
    );

    let log_operations_a = node_a.process_announcement(announcement_b, topic).await?;

    println!("--------");

    // Node B processes the operations from node A.

    // We only expect operations at this stage; no more announcements.
    for operation in log_operations_a {
        println!(
            "{}: ingest remote operation {}",
            node_b.id().fmt_short(),
            operation.hash().fmt_short()
        );

        node_b.ingest_operation(topic, operation).await?;
    }

    println!("--------");

    // Live mode.
    //
    // Node B directly sends a new operation to node A without first sending an updated announcement.
    let operation_b = tx!(node_b.store, {
        let log_id = LogId::digest(topic.as_bytes());

        let operation = log_b.operation(b"we're in sync!", CustomExtensions { log_id });

        <SqliteStore as OperationStore<Operation<CustomExtensions>, Hash>>::insert_operation(
            &node_b.store,
            &operation.hash,
            &operation,
            &log_id,
        )
        .await?;

        AnyOperation {
            hash: operation.hash,
            header: operation
                .header
                .try_into()
                .expect("shouldn't be an error in p2panda-core"),
            body: operation.body,
        }
    });

    // Node A processes the new operation.
    println!(
        "{}: ingest remote live operation {}",
        node_a.id().fmt_short(),
        operation_b.hash().fmt_short()
    );

    node_a.ingest_operation(topic, operation_b).await?;

    Ok(())
}

// TODO: This function may be a good candidate for inclusion in the `p2panda-store` API.
async fn get_topic_log_heights(
    store: &SqliteStore,
    topic: &Topic,
) -> Result<LogHeights<VerifyingKey, LogId>> {
    let logs: LogIds = store.resolve(topic).await?;
    let log_heights = get_log_heights(&store, &logs).await?;

    Ok(log_heights)
}

// TODO: This function may be a good candidate for inclusion in the `p2panda-store` API.
async fn get_log_heights(
    store: &SqliteStore,
    logs: &LogIds,
) -> Result<LogHeights<VerifyingKey, LogId>> {
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
