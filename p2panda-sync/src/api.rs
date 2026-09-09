// SPDX-License-Identifier: MIT OR Apache-2.0

//! Methods for building sync protocols using p2panda's [append-only log](p2panda_core::operation)
//! for a wide range of network topologies or transports.
//!
//! This module contains all essential methods, types and helpers for querying local log state from
//! the store, computing and comparing log state vectors, validating operation log integrity,
//! out-of-order buffering, writing incoming operations into the store and more.
//!
//! ## Examples
//!
//! See our [examples](/examples) folder for inspiration on how to achieve sync over a regular
//! unicast connection or mesh networks with broadcast topologies, such as LoRa, BLE Advertisements
//! or store-and-forward, delay-tolerant USB-stick sneakernets.
//!
//! ## What is a sync protocol?
//!
//! Broadly speaking any _sync_ or _replication_ protocol assures that all involved nodes in a
//! network will receive the same data, even if some of them have been offline for a while. Latest
//! when two nodes connect and _sync_, they'll exchange to each other what each party had missing so
//! far.
//!
//! ```text
//! Node A replica:
//! [A] [B] [C]
//!                                 Node B replica:
//!                                 [A] [D] [E]
//!
//!              Sync Protocol begins
//!
//!                  <--- [D] [E]
//!
//!                  [B] [C] --->
//!
//!               Sync Protocol ends
//!
//! => Node A & B have the same state now: [A] [B] [C] [D] [E]
//! ```
//!
//! The local state of a node is called a _replica_. It is usually a sub-set of all data in the
//! network. With a sync protocol replicas want to converge to the same state. This characteristic
//! we call _eventual consistency_ which essentially means: All replicas will converge to the same
//! state after "some" time, of course given that they had a chance to exchange data at some point.
//! Eventual consistency is one of the core guarantees of any local-first system and therefore we
//! always need a sync protocol.
//!
//! To achieve eventual consistency we do not only need a sync protocol but also a CRDT
//! (Conflict-Free Replicated Data-Yype) which has the characteristic to make everyone
//! deterministically _converge_ to the same state after all changes to the replicas have been
//! observed by everyone. In p2panda we call this a _Convergent Data Type_ (CDT) to isolate the term
//! a bit more from CRDTs who have become more overloaded. CDTs are usually simple Last-Write-Wins
//! sets- or key-value ranges, Grow-Only sets or Append-Only Logs.
//!
//! A naive, very spammy sync protocol would just send everything all the time and eventually
//! everyone received everything. To make things more efficient replicas usually compute & exchange
//! state vectors first, informing others about their local replica state. The receiving nodes
//! compute the missing difference (the union of both replica's sets) and sends over only the
//! missing data. If all replicas reached the same state, no data is exchanged anymore.
//!
//! Sync protocols want to usually optimize towards different aspects: Is it cheap to compute the
//! state and difference? Does it consume little memory? How many messages need to be sent before we
//! can sync actual data? Can I sync only a partial of the data when I don't need everything?
//!
//! Another aspect is the security of the protocol: Can we make sure that the data sent to me is
//! also what I asked for or can an attacker exhaust all my computer resources before I've even
//! noticed that?
//!
//! In p2panda we've focused on append-only logs as the base convergent data type which delivers all
//! application data (which again can be shaped in any other way). We've chose optimising for as
//! little messaging overhead as possible to compare state efficiently and allowing sync protocols
//! which do not assume a point-to-point / unicast connection.
//!
//! ## Log-height sync
//!
//! p2panda uses append-only logs as the data-type, we call an entry in the log an
//! [`Operation`](p2panda_core::operation). Every operation in the log is labeled with a sequence
//! number [`SeqNum`], starting at zero. Building a sync protocol on top of logs is
//! straight-forward: Nodes share their latest sequence number which is enough for any other node to
//! compute the difference and send over the missing operations.
//!
//! ```text
//! Node A replica:
//! [0] <- [1] <- [2]
//!                                 Node B replica:
//!                                 [0] <- [1]
//!
//!              Sync Protocol begins
//!
//!          <--- "I have log height [1]"
//!
//!                  [2] --->
//!
//!               Sync Protocol ends
//!
//! => Node A & B have the same state now: [0] <- [1] <- [2]
//! ```
//!
//! **Log heights as state vectors**
//!
//! The state vector of append-only logs are the [`LogHeights`] which indicate the HEAD (in git
//! terms) or frontier by stating the latest [`SeqNum`]. If more logs need exchanging the log
//! heights are labelled with an unique [`LogId`].
//!
//! In p2panda we usually sync over [`Topic`](p2panda_core::Topic) which describe some sort of
//! application data. Internally topics are collections of
//! [associated](p2panda_store::topics::TopicStore ) multiple logs of many authors.
//!
//! **Query local log heights**
//!
//! To compute all associated log state vectors from a topic use the [`topic_log_heights`] method or
//! [`log_heights`] to compute the state vectors from a list of log ids.
//!
//! **Computing difference between log heights**
//!
//! When receiving a remote state vector of another replica you want to use [`compare_logs`] to
//! compute the difference with your local state. The resulting difference is [`LogRanges`] which
//! can be used to query all missing operations for the other node from your database.
//!
//! **Query missing operations**
//!
//! You can use the [`log_ranges`] method to efficiently stream all operations from all logs given
//! the computed difference. This is the data you finally want to send "over the wire" with your
//! sync protocol.
//!
//! **Ingest received operations**
//!
//! Use [`ingest_operation`] to validate incoming operations and insert them into your local store.
//! Use an out-of-order buffer [`OooBuffer`] if you can't guarantee that these operations will come
//! in correct log order (monotonically incrementing sequence numbers) depending on your sync
//! approach.
//!
//! **Announcing your local state**
//!
//! [`LogHeights`] state vectors can be delivered in many ways, for example during a regular sync
//! session over a direct bi-directional connection with another node or broadcast in a radio
//! network. Whoever will receive this information can respond with the missing operations after
//! computing the difference. This is how p2panda achieves sync functionality across various
//! transports.
//!
//! ## Design considerations building a protocol
//!
//! ### Validation
//!
//! Sync protocols might want to validate the operation format, log-integrity or even application
//! data already during sync before it even reaches the application layer. The [`ingest_operation`]
//! method handles this part and can be enabled with the `ingest` feature flag. If no insertion into
//! the database is necessary one can also use the [`validate_operation`] method only.
//!
//! In p2panda's sync protocol implementations we chose to move validation into the event processing
//! layer which happens outside of the sync- or _event delivery_ layer. Sync protocols only
//! _forward_ incoming operations to any higher layer:
//!
//! ```text
//! Event Delivery               Event Processing
//! ==============               ================
//!
//! Sync + Transport     -->     Decryption -> Validation -> Insertion
//!                                             |
//!           <----------------------- Report back on error
//!
//!                      vs.
//!
//! Sync + Transport     -->     Insertion, etc.
//!   + Validation
//! ```
//!
//! Like this we can clearly separate these concern for maximum flexibility. If data is fully
//! encrypted, for example in support nodes or store-and-forward buffers, this separation becomes
//! even necessary, since the sync layer might not be aware of any encryption secrets, attempting
//! decryption during sync might also be too slow. However this separation implies that any higher
//! processing layer needs to inform the sync protocol about potential violations or errors.
//!
//! Broker nodes might want to validate already during sync if it is readable to them, so they can
//! be sure to only stream validated data further to clients.
//!
//! ### Fast-push vs. slow repair
//!
//! Sync can be seen as a form of _repairing_ messages we've missed due to being offline,
//! reachability or networking issues. In comparison to just sending out every new message
//! ("Fast-push") into the network this is a more expensive operation ("Slow-Repair").
//!
//! **Sync first, then live-mode**
//!
//! If the connection is cheap and fast (reliable internet with enough bandwidth) we can easily sync
//! first and then eagerly and fastly push any new messages which have been created after the sync
//! session has ended without starting to start sync again. In p2panda we also call this _live
//! mode_.
//!
//! **Fast-push first, then sync**
//!
//! In some network topologies or transports syncing might not be the first option due to bandwidth
//! or message throughput restrictions (see NACK-implosion making things worse). In these cases we
//! might want to optimistically always flood latest messages, even if nodes might be out-of-sync.
//! If nodes detect that they are very far behind they might initiate a sync session ("slow
//! repair"). Additionally nodes probably want to utilise some sort of [`OooBuffer`] to handle
//! operations coming out-of-order.
//!
//! **NACK-implosions**
//!
//! In reliable multicast networking protocols we call NACK the request to re-transmit packets which
//! have been missed and _not_ acknowledged yet. This is very similar to a sync protocol.
//!
//! NACKing comes with it's own issues though: It can lead to spamming the network when multiple
//! participants start announcing what they've missed and everyone starts streaming back the same
//! missing operations. This is what we call a NACK-implosion.
//!
//! For some protocols we want to mitigate these issues with _not_ responding based on a density
//! factor `d` or similar, however this implies that slow repair or sync becomes more rare in
//! certain networks.
//!
//! ### Out-of-order buffering
//!
//! Not every transport will guarantee an ordered connection (such as TCP) and data might arrive
//! out-of-order. This can also happen if we eagerly always publish operations without syncing first
//! (see _Fast-Push_).
//!
//! For this we want to buffer operations until the log-integrity became intact again.
//! Usually we want to keep this buffer only in-memory with a fixed bound to prevent attacks. In
//! some scenarios with extreme delay-tolerance and out-of-order behavior we might want to persist
//! the buffer. See [`OooBuffer`] for a ring-buffer implementation handling out-of-order operations.
//!
//! ### Partial replication
//!
//! p2panda supports partial replication in a rather simple way: Authors can publish application
//! data in multiple logs and a sync protocol can choose to only replicate a sub-set of them.
//!
//! ```text
//! Author "panda":
//! - "trees" log
//! - "animals" log       -> Sync Protocol only requests all "trees" logs
//!
//! Author "icebear":
//! - "animals" log
//! ```
//!
//! Partial sync of a single log is only possible if the log does not contain any gaps due to the
//! strict integrity guarantees of an append-only log. This means that in worst-case the log needs
//! to be replicated from the beginning until a desired point n.
//!
//! ```text
//! [0] <- [1] <- [2]    -> Sync protocol only requests range [0..1]
//! ```
//!
//! Applications usually prune their logs and in practice we haven't observed long-growing logs
//! which would require partial replication. In combination with selecting which logs to sync we
//! believe these are enough facilities to express most application's needs in terms of partial
//! replication:
//!
//! ```text
//! Multiple, pruned logs:
//!     <- [2]
//!         ^
//!     Pruning point
//!
//! <- [8] <- [9]
//!     ^
//!  Pruning point
//! ```
//!
//! If your sync protocol requires syncing only ranges of log, even if no pruning is involved you
//! will have to break the consistency guarantee of the log itself, which will require custom
//! validation logic:
//!
//! ```text
//! [0] <- .. <- [192] <- [193]    -> Sync protocol only requests range [99..102]
//!
//! Validation:
//!
//!     ... <- [99] <- [100] <- [101] <- [102]
//!      ^
//! Missing backlink
//! ```
//!
//! We wouldn't recommend such an approach and rather suggest considering modeling your more complex
//! data-types _on top_ of append-only logs. Like this you can express your custom logic on top of
//! logs as you would layer protocols on top of IP packets.
//!
//! If you still need more efficient partial sync, you might want to use your own base convergent
//! data-type.
//!
//! ### Fork-tolerance
//!
//! ### Pruning
//!
//! ### Meta-data & privacy
//!
//! ### Topic association
//!
//! ### Bandwidth & message throughput
//!
//! ### Integration with `p2panda` processing layer
//!
use futures_util::stream;
use futures_util::{StreamExt, future};
pub use p2panda_core::logs::{LogHeights, LogRanges, Logs, compare_logs};
use p2panda_core::{AnyOperation, Hash, LogId, SeqNum, VerifyingKey};
use p2panda_store::logs::LogStore;
use p2panda_store::topics::TopicStore;
#[cfg(feature = "ingest")]
pub use p2panda_stream::ingest::{
    IngestError, IngestResult, OooBuffer, OooResult, ingest_operation,
};
use thiserror::Error;

/// Item delivered from a [`LogStream`].
pub type LogEntry<L> = p2panda_store::logs::LogEntry<AnyOperation, L>;

/// Stream of operations in a log range.
pub type LogStream<L, E> = p2panda_store::logs::LogStream<AnyOperation, L, E>;

/// Compute log heights of all passed author logs based on what is known in the local store.
pub async fn log_heights<L, S>(
    store: &S,
    logs: &Logs<VerifyingKey, L>,
) -> Result<LogHeights<VerifyingKey, L>, S::Error>
where
    L: LogId,
    S: LogStore<AnyOperation, VerifyingKey, L, SeqNum, Hash>,
{
    let mut result = LogHeights::new();
    for (verifying_key, log_ids) in logs {
        let Some(log_heights) = store.get_log_heights(verifying_key, log_ids).await? else {
            continue;
        };
        result.insert(*verifying_key, log_heights);
    }

    Ok(result)
}

/// Construct a stream which returns all operations in the provided log ranges.
///
/// This is a memory efficient query with only one stream item staying in memory at a time. If any
/// error occurs when fetching items the stream will return the error then immediately close.
pub fn log_ranges<L, S>(store: &S, ranges: LogRanges<VerifyingKey, L>) -> LogStream<L, S::Error>
where
    L: LogId + Send + 'static,
    S: LogStore<AnyOperation, VerifyingKey, L, SeqNum, Hash> + Clone + Send + 'static,
{
    let mut flattened_ranges = vec![];
    for (author, log_heights) in ranges {
        for (log_id, (after, until)) in log_heights {
            flattened_ranges.push((author, log_id, after, until));
        }
    }

    let store = store.clone();

    let stream = stream::iter(flattened_ranges)
        // Flatten all inner streams into one.
        .flat_map(move |(author, log_id, after, until)| {
            match store.log_entries(&author, &log_id, after, until) {
                Ok(stream) => stream,
                // If an error occurs when constructing the next stream, return a new stream
                // containing one item, the error itself.
                Err(err) => Box::pin(stream::once(async move { Err(err) })),
            }
        })
        // If any error is observed then immediately close the stream.
        .scan(false, |error_occurred, item| {
            if *error_occurred {
                return future::ready(None);
            }
            *error_occurred = item.is_err();
            future::ready(Some(item))
        });

    Box::pin(stream)
}

/// Compute log heights for the given topic based on what is known in the local store.
pub async fn topic_log_heights<L, S, T>(
    store: &S,
    topic: &T,
) -> Result<LogHeights<VerifyingKey, L>, StoreError<L, S, T>>
where
    L: LogId,
    S: TopicStore<T, VerifyingKey, L> + LogStore<AnyOperation, VerifyingKey, L, SeqNum, Hash>,
{
    let logs: Logs<VerifyingKey, L> = store
        .resolve(topic)
        .await
        .map_err(|err| StoreError::TopicStore(err))?;
    let log_heights = log_heights(store, &logs)
        .await
        .map_err(|err| StoreError::LogStore(err))?;

    Ok(log_heights)
}

/// Critical store failure.
#[derive(Debug, Error)]
pub enum StoreError<L, S, T>
where
    L: LogId,
    S: TopicStore<T, VerifyingKey, L> + LogStore<AnyOperation, VerifyingKey, L, SeqNum, Hash>,
{
    #[error(transparent)]
    LogStore(<S as LogStore<AnyOperation, VerifyingKey, L, SeqNum, Hash>>::Error),

    #[error(transparent)]
    TopicStore(<S as TopicStore<T, VerifyingKey, L>>::Error),
}
