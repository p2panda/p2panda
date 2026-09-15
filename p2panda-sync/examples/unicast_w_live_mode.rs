// SPDX-License-Identifier: MIT OR Apache-2.0

//! Sync over point-to-point (unicast) session with eager push.
//!
//! This example shows how a sync protocol can be built for any point-to-point connection-based
//! transport using p2panda append-only logs and helper methods. The protocol showcased here is
//! very similar to the log sync protocol used in `p2panda-net` (and in the high-level `p2panda`
//! node, by extension). Rather than syncing over all overlapping topics of interest in one session,
//! this protocol takes place in the context of a single topic. The topic over which the protocol is
//! being run is therefore assumed to have been negotiated beforehand.
//!
//! The protocol can be said to include two phases: 1) catch up on any missing state by exchanging
//! state vectors and operations, 2) eagerly push new operations. One might also choose to build a
//! protocol which takes the reverse approach: 1) eagerly push new operations (with the receiver
//! storing them in an out-of-order buffer), 2) catch up on any missing state if this is explicitly
//! requested.
//!
//! In our example we pass messages directly between nodes to keep things simple. A real-world
//! implementation might run over a TCP connection or iroh's QUIC streams.
//!
//! ## Protocol
//!
//! The prerequisite for running the protocol is to establish a connection and decide on the
//! session topic.
//!
//! 1. Alpaca generates an announcement message which encodes the log heights they hold for logs
//!    associated with the topic of interest. The message is sent to Bobcat.
//! 2. Bobcat compares the log heights in the received announcement to their own state, calculates
//!    the difference and sends the required operations to Alpaca.
//! 3. Bobcat generates their own announcement message and sends it to Alpaca.
//! 4. Alpaca ingests the operations received from Bobcat.
//! 5. Alpaca compares the log heights in the received announcement to their own state, calculates
//!    the difference and sends the required operations to Bobcat.
//! 6. Now that both Alpaca and Bobcat have caught up on what they're missing they enter into the
//!    eager push phase of the protocol, aka. live-mode. Any newly created or received operation
//!    which is inferred to be of interest to the other node is sent immediately, without the need
//!    for a preceeding announcement message. In the example we illustrate this by Bobcat creating
//!    a new operation and immediately sending it to Alpaca.
mod common;

use std::collections::BTreeMap;

use futures_util::stream::StreamExt;
use p2panda_core::traits::Digest;
use p2panda_core::{AnyOperation, Hash, Operation, SeqNum, SigningKey, Topic, VerifyingKey};
use p2panda_store::{SqliteError, SqliteStore};
use p2panda_sync::api::{LogStream, compare_logs, ingest_operation, log_ranges, topic_log_heights};
use p2panda_sync::protocols::ShortFormat;

use crate::common::{CustomExtensions, LogId, create_operation};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// Announcement message containing a set of log heights.
///
/// The announcement represents the current state held by a node or replica for all logs associated
/// with a particular topic (ie. the topic over which the sync session is being run). It can be
/// framed as a HAVE message, where the sender is essentially communicating: "This is all the data I
/// have for this topic, please send me anything I'm missing".
///
/// The responder may send operations for logs which the sender is not yet aware of (ie. a log which
/// was not in the announcement set but which the responder has associated with the topic) or which
/// advance the state of known logs (by providing more recently published operations).
///
/// Other protocols might wish to include the topic and the verifying key of the message author as
/// part of the announcement. We omit those field here because they are known from the context of
/// the sync session (which runs over a single topic with a known partner).
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
    async fn populate_log(&self, topic: Topic) -> Result<()> {
        for op_i in 0..5 {
            let body = (op_i as usize).to_be_bytes();
            self.create_operation(topic, &body).await?;
        }

        Ok(())
    }

    async fn create_operation(&self, topic: Topic, body: &[u8]) -> Result<AnyOperation> {
        let operation = create_operation(&self.store, &self.signing_key, topic, &body).await?;
        Ok(operation)
    }

    /// Query the local log heights for the topic and return an announcement.
    async fn generate_announcement(&self, topic: Topic) -> Result<Announcement> {
        let log_heights = topic_log_heights(&self.store, &topic).await?;

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
    ) -> Result<LogStream<LogId, SqliteError>> {
        let their_log_heights = &announcement.log_heights;
        let our_log_heights = topic_log_heights(&self.store, &topic).await?;

        Ok(log_ranges(
            &self.store,
            compare_logs(&our_log_heights, &their_log_heights),
        ))
    }

    /// Insert an operation into the store and associate the log with the given topic.
    async fn ingest_operation(&self, topic: Topic, operation: AnyOperation) -> Result<()> {
        // 1. Check if custom header extensions matches expected format.
        // TODO: Clone can be removed after OooBuffer PR was merged.
        let operation = Operation::<CustomExtensions>::try_from(operation.clone())?;

        // 2. Check if claimed log id matches topic.
        let log_id_check = LogId::digest(topic.as_bytes());
        if log_id_check != operation.header.extensions.log_id {
            return Err("log id does not match topic digest".into());
        }

        // 3. Validate and store operation in database.
        ingest_operation(
            &self.store,
            None,
            &operation,
            &operation.header.extensions.log_id,
            &topic,
            false,
        )
        .await?;

        Ok(())
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    let topic = Topic::random();

    // Alpaca and Bobcat populate their logs with operations.

    let alpaca = Node::new().await;
    alpaca.populate_log(topic).await?;
    println!("{}: alpaca", alpaca.id().fmt_short());

    let bobcat = Node::new().await;
    bobcat.populate_log(topic).await?;
    println!("{}: bobcat", bobcat.id().fmt_short());

    println!("--------");

    // Alpaca sends an announcement to Bobcat.

    let alpaca_announcement = alpaca.generate_announcement(topic).await?;

    println!(
        "{}: process remote announcement {}",
        bobcat.id().fmt_short(),
        alpaca_announcement.hash().fmt_short()
    );

    // Bobcat processes the announcement then sends operations and an announcement.

    let mut bobcat_log_operations = bobcat
        .process_announcement(alpaca_announcement, topic)
        .await?;
    let bobcat_announcement = bobcat.generate_announcement(topic).await?;

    println!("--------");

    // Alpaca processes the operations and announcement from Bobcat then sends operations.

    while let Some(Ok(log)) = bobcat_log_operations.next().await {
        let operation = log.entry;

        println!(
            "{}: ingest remote operation {}",
            alpaca.id().fmt_short(),
            operation.hash().fmt_short()
        );

        alpaca.ingest_operation(topic, operation).await?;
    }

    println!(
        "{}: process remote announcement {}",
        alpaca.id().fmt_short(),
        bobcat_announcement.hash().fmt_short()
    );

    let mut alpaca_log_operations = alpaca
        .process_announcement(bobcat_announcement, topic)
        .await?;

    println!("--------");

    // Bobcat processes the operations from Alpaca.

    // We only expect operations at this stage; no more announcements.
    while let Some(Ok(log)) = alpaca_log_operations.next().await {
        let operation = log.entry;

        println!(
            "{}: ingest remote operation {}",
            bobcat.id().fmt_short(),
            operation.hash().fmt_short()
        );

        bobcat.ingest_operation(topic, operation).await?;
    }

    println!("--------");

    // Live mode.
    //
    // Bobcat sends a new operation to Alpaca without first sending an updated announcement.
    let bobcat_operation = bobcat.create_operation(topic, b"we're in sync!").await?;

    // Alpaca processes the new operation.
    println!(
        "{}: ingest remote live operation {}",
        alpaca.id().fmt_short(),
        bobcat_operation.hash().fmt_short()
    );

    alpaca.ingest_operation(topic, bobcat_operation).await?;

    Ok(())
}
