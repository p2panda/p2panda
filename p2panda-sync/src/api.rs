// SPDX-License-Identifier: MIT OR Apache-2.0

#![allow(
    rustdoc::redundant_explicit_links,
    reason = "rust docs can't resolve links for SeqNum and LogId types due to a bug"
)]

//! Methods for building sync protocols using p2panda's [append-only log](p2panda_core::operation)
//! for a wide range of network topologies and transports.
//!
//! This module contains all essential methods, types and helpers for querying local log state from
//! the store, computing and comparing log state vectors, validating operation log integrity,
//! buffering out-of-order operations, reading and writing operations and more.
//!
//! ## Examples
//!
//! See our [examples](https://github.com/p2panda/p2panda/tree/main/p2panda-sync/examples) folder
//! for inspiration on how to achieve sync over a regular unicast connection or meshes with
//! broadcast topologies, such as LoRa, BLE Advertisements, sneakernets or store-and-forward,
//! delay-tolerant networks.
//!
//! ## Integration with `p2panda` processing layer
//!
//! p2panda strictly separates the _event delivery_ (`p2panda-net` and `p2panda-sync`) from the
//! _event processing_ layer (`p2panda-stream`) while [`p2panda`](https://crates.io/crates/p2panda)
//! is the out-of-the-box combination of both worlds.
//!
//! If you want to use your own sync protocol with `p2panda` you can currently establish an active
//! topic handle and use the
//! [`import`](https://docs.rs/p2panda/latest/p2panda/streams/struct.StreamPublisher.html#method.import)
//! method to import your synced data into the topic stream with all features the high-level API
//! brings.
//!
//! ## What is a sync protocol?
//!
//! Broadly speaking _sync_ or _replication_ protocols ensure that all involved nodes in a network
//! will receive the same data, even if some of them have been unreachable for a while. Once nodes
//! connect and _sync_, they'll exchange the data that each party has been missing.
//!
//! ```text
//! Node A replica:
//! [A] [B] [C]
//!                                 Node B replica:
//!                                     [A] [D] [E]
//!
//!              [Sync Protocol begins]
//!
//!                  <--- [D] [E]
//!
//!                  [B] [C] --->
//!
//!               [Sync Protocol ends]
//!
//! => Node A & B have the same state now: [A] [B] [C] [D] [E]
//! ```
//!
//! The local state of a node is called a _replica_. It can be seen as a subset of "all data" in the
//! network. With a sync protocol replicas want to converge to the same state. This underlying
//! characteristic we call _eventual consistency_ which essentially means: All replicas will
//! converge to the same state after "some" time, of course given that they have a chance to
//! exchange data at some point. Eventual consistency is one of the core guarantees of any
//! local-first system and therefore we always need a sync protocol.
//!
//! A naive, very spammy sync protocol would send everything all the time and eventually everyone
//! would have received everything. To make things more efficient, replicas usually compute and
//! exchange so called _state vectors_ first, informing others about their local replica state. The
//! receiving nodes compute the difference between both replica's sets and send over only the
//! missing data. Once all replicas have reached the same state, no data is exchanged anymore.
//!
//! **Different sync protocols, different characteristics**
//!
//! Sync protocols want to usually optimise towards different aspects: Is it inexpensive to compute
//! the state and difference? Does it consume little memory? How many messages need to be sent
//! before we can sync actual data? Can I sync only a subset of the data when I don't need
//! everything?
//!
//! Another aspect is the security of the protocol: Can we make sure that the data sent to us is
//! also what we asked for, is it guaranteed to be correct, or can an attacker exhaust all our
//! computer resources before we've even noticed it?
//!
//! **Convergent data types**
//!
//! To achieve eventual consistency we do not only need a sync protocol but also a CRDT
//! ([Conflict-free replicated data
//! type](https://en.wikipedia.org/wiki/Conflict-free_replicated_data_type)) which has the
//! characteristic to make everyone deterministically _converge_ to the same state after all changes
//! to the replicas have been observed by everyone. In p2panda we call this a _Convergent Data Type_
//! (CDT) to isolate the term a bit more from CRDTs, which have become increasingly overloaded. CDTs
//! are usually simple Last-Write-Wins Sets or LWW-Key-Value Ranges, Grow-Only-Sets or Append-Only
//! Logs.
//!
//! **Why append-only logs?**
//!
//! In p2panda we've decided on append-only logs as the base convergent data type which delivers all
//! application data. Application data can be shaped in any way and developers can express more
//! advanced data types on top of logs via the [`Extensions`](p2panda_core::Extensions) or
//! [`Body`](p2panda_core::Body), for example
//! [DAGs](https://en.wikipedia.org/wiki/Directed_acyclic_graph) as we do for [causal
//! ordering](https://docs.rs/p2panda-stream/latest/p2panda_stream/orderer/index.html) across
//! multiple logs, [log-prefix
//! pruning](https://docs.rs/p2panda-stream/latest/p2panda_stream/log_prune/index.html) or
//! [Byzantine-Fault tolerance](https://arxiv.org/abs/2307.08381).
//!
//! We've chosen this to minimise messaging overhead (one message to exchange the state vector),
//! enable efficient state comparison (constant time), and support sync protocols that do not assume
//! a point-to-point (unicast) connection, especially where message throughput may be limited.
//!
//! Additionally, logs neatly represent the event sourcing aspects of peer-to-peer applications due
//! to their natural representation of history and total ordering guarantees. In applications we
//! usually observe a stream of _events_ to materialise application state.
//!
//! The research and experiments in this area are not exhausted and [interesting work is
//! happening](https://github.com/ssbc/tinySSB) around using append-only logs.
//!
//! **Hourglass model**
//!
//! An entry in a log, the base convergent data type, can be also understood as the _narrow waist_,
//! similar to the [Hourglass model](https://en.wikipedia.org/wiki/Hourglass_model) in computer
//! networking. The sync protocol and log data type define the most atomic unit of delivery of data,
//! only fulfilling that particular concern (eventual consistency) from which more complex layers
//! can be expressed on top.
//!
//! ## Log-height sync
//!
//! p2panda uses append-only logs as the base convergent data type. It is the unit to be synced. We
//! call an entry in the log an [`Operation`](p2panda_core::operation). Every operation in the log
//! is labeled with a sequence number [`SeqNum`](p2panda_core::SeqNum), starting at zero. Building a
//! sync protocol on top of logs is straight-forward: Nodes share their latest sequence number in
//! the form of a state vector which is enough for any other node to compute the difference and send
//! over the missing operations.
//!
//! ```text
//! Node A replica:
//! [0] <- [1] <- [2]
//!                                 Node B replica:
//!                                      [0] <- [1]
//!
//!              [Sync Protocol begins]
//!
//!                       (1) Node B computes state
//!                        vector and announces it:
//!
//!          <--- "I have log height [1]"
//!
//! (2) Node A computes diff:
//!
//! [0] <- [1] <- [2]
//!         ^
//!    Remote state
//!
//! (3) Node A streams back missing operations for B:
//!
//!                    [2] --->
//!
//!                  (4) Node B validates operations
//!                  and inserts them into database.
//!
//!               [Sync Protocol ends]
//!
//! => Node A & B have the same state now: [0] <- [1] <- [2]
//! ```
//!
//! **Log heights as state vectors**
//!
//! The state vector of append-only logs is the [`LogHeights`] which indicate the HEAD (in git
//! terms) or _frontier_ by stating the latest [`SeqNum`](p2panda_core::SeqNum). If more logs need
//! to be exchanged the log heights are labelled with a unique
//! [`Author`](p2panda_core::traits::Author) and [`LogId`](p2panda_core::LogId) tuple.
//!
//! **Topic -> log map**
//!
//! In p2panda we usually sync over [`Topic`](p2panda_core::Topic) which describes some sort of
//! application data of interest to that node. Internally topics are collections of logs. We
//! actively need to [associate](p2panda_store::topics::TopicStore::associate) new logs to topics
//! when creating or receiving them to keep the topic -> log mapping up-to-date.
//!
//! **Query and announce local log heights**
//!
//! > _Step (1) in figure above._
//!
//! To compute all associated log state vectors from a topic use the [`topic_log_heights`] method or
//! [`log_heights`] to compute the state vectors from a list of log ids.
//!
//! [`LogHeights`] state vectors can be delivered in many ways, for example during a regular sync
//! session over a direct bi-directional connection with another node or broadcast in a radio
//! network. Whoever will receive this information can respond with the missing operations after
//! computing the difference. This is how p2panda achieves sync functionality across various
//! transports.
//!
//! **Computing difference between log heights**
//!
//! > _Step (2) in figure above._
//!
//! When receiving a remote state vector of another replica you want to use [`compare_logs`] to
//! compute the difference with your local state. The resulting difference is [`LogRanges`] which
//! can be used to query all missing operations for the other node from your database.
//!
//! **Query missing operations**
//!
//! > _Step (3) in figure above._
//!
//! You can use the [`log_ranges`] method to efficiently stream all operations from all logs given
//! the computed difference. This is the data you finally want to send "over the wire" with your
//! sync protocol.
//!
//! **Ingest received operations**
//!
//! > _Step (4) in figure above._
//!
//! Use [`ingest_operation`] to validate incoming operations and insert them into your local store.
//! Use an out-of-order buffer [`OooBuffer`] if you can't guarantee that these operations will come
//! in correct log order (monotonically incrementing sequence numbers) depending on your sync
//! approach.
//!
//! ## Design considerations building a protocol
//!
//! When building your own sync protocol using p2panda's log data type, you might want to handle
//! things differently than us. Here is a list of design considerations we've encountered when
//! building different sync strategies which are helpful. This list is not exhaustive and we are
//! sure there's many more creative ways to play with log-sync protocols!
//!
//! ### Validation
//!
//! Sync protocols might want to validate the operation format, log-integrity or even application
//! data already during sync before it even reaches any higher layer. The [`ingest_operation`]
//! method handles this part and can be enabled with the `ingest` feature flag. If no insertion into
//! the database is necessary one can also use the [`validate_operation`] method.
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
//! This allows us to clearly separate these concerns for maximum flexibility. If data is fully
//! encrypted, for example in support nodes or store-and-forward buffers, this separation becomes
//! necessary, since the sync layer might not be aware of any encryption secrets or the process runs
//! on a separate device. However, this separation implies that any higher processing layer needs to
//! inform the sync protocol about potential violations or errors on-the-fly.
//!
//! Broker nodes might want to validate already during sync if the data is readable to them, so they
//! can be sure to only stream validated data further to clients.
//!
//! ### Fast-push vs. sync
//!
//! Sync can be seen as a strategy of _catching-up_ on messages we've missed due to being offline or
//! networking issues. In comparison to just sending out every new message as soon as it was created
//! ("Fast-push") into the network sync is a more expensive operation. We can switch between these
//! two modes in any order with different trade-offs:
//!
//! **Sync first, then fast-push**
//!
//! If the connection is cheap and fast (reliable internet with enough bandwidth) we can easily sync
//! first and then eagerly push any new messages which have been created after the sync session has
//! ended without starting to sync again. In p2panda we also call this _live mode_.
//!
//! Note that if any message was pushed eagerly to us _while_ we are syncing with that node, we need
//! to buffer them (FIFO) and flush _after_ the sync session has ended to preserve ordering.
//!
//! **Fast-push first, then sync**
//!
//! In some network topologies or transports syncing might not be the first option due to bandwidth
//! or message throughput restrictions. See NACK-implosion making things worse.
//!
//! In these cases we might want to optimistically always _flood_ the latest messages, even if nodes
//! might be out-of-sync. If nodes detect that they are very far behind they might initiate a sync
//! session _lazily_ ("slow repair"). Additionally nodes probably want to utilise some sort of
//! [`OooBuffer`] to handle operations coming out-of-order.
//!
//! **NACK-implosions**
//!
//! In reliable multicast networking protocols we can understand
//! [NACK](https://en.wikipedia.org/wiki/NACK-Oriented_Reliable_Multicast) as a request to
//! retransmit packets which have been missed and _not_ acknowledged yet. This is very similar to a
//! sync protocol.
//!
//! NACKing comes with its own issues: It can lead to spamming the network when multiple
//! participants start announcing what they've missed and everyone starts streaming back the same
//! missing operations until the network breaks down. This is what we call a _NACK-implosion_.
//!
//! For some protocols we want to mitigate these issue by _not_ always responding. One way of
//! achieving this is to calculate a response probability in relation to the number of nearby
//! network participants (ie. a density factor `d`); the greater the number of detected neighbours,
//! the lower the chance of rebroadcasting a received message. This implies that sync can become
//! slower.
//!
//! ### Out-of-order buffering
//!
//! Not every transport will guarantee an ordered connection (such as TCP) and data might arrive
//! out-of-order. This can also happen if we eagerly publish operations without syncing first (see
//! _Fast-push_).
//!
//! For this we want to buffer operations until the log-integrity is intact again. Usually we want
//! to keep this buffer only in-memory with a fixed bound to prevent attacks which maliciously
//! exhaust system resources with out-of-order data. In some scenarios with extreme delay-tolerance
//! and out-of-order behavior we might want to persist the buffer. See [`OooBuffer`] for a
//! ring-buffer implementation handling out-of-order operations.
//!
//! ### Partial replication
//!
//! p2panda supports partial replication in a rather simple way: Authors can publish application
//! data in multiple logs and a sync protocol can choose to only replicate a subset of logs.
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
//! strict integrity guarantees of an append-only log. This means that in worst case the log needs
//! to be replicated from the beginning until a desired point `n`.
//!
//! ```text
//! [0] <- [1] <- [2]    -> Sync protocol only requests range [0..1]
//! ```
//!
//! However, applications usually prune their logs and in practice we haven't observed long-growing
//! logs which would require partial replication. In combination with selecting which logs to sync
//! we believe these are enough facilities to express most applications' needs in terms of partial
//! replication.
//!
//! If your sync protocol requires syncing only _the latest_ information, for example the latest `n`
//! items of a log it will have to break the consistency guarantee of the log itself, which will
//! require custom validation logic:
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
//! In this case it would make more sense to design your logs in a way where one log represents for
//! example one day of data being created. Instead of choosing to partially replicate _within_ the
//! log you can now partially replicate the logs for the days you are interested in, for example the
//! latest day:
//!
//! ```text
//! - "trees 02.03.2026" log
//! ...
//! - "trees 09.03.2026" log  -> Sync protocol only requests "trees" logs >= 09.03.2026
//! - "trees 10.03.2026" log
//! ```
//!
//! If you still need more specific partial sync strategy, you might want to use your own base
//! convergent data type or extend the existing ones, for example a merkle-tree or skip-list on top
//! of a log which would be possible to express in the form of header
//! [`Extensions`](p2panda_core::Extensions).
//!
//! ### Content Addressing
//!
//! p2panda usually uses [`Topic`](p2panda_core::Topic) to group append-only logs into a collection
//! which is a form of content addressing. Applications _subscribe_ to topics to enable sync for
//! this particular topic of interest. From now on they will receive logs grouped under this topic.
//!
//! The kind of addressing and mapping is generic in p2panda (usually expressed as a type `T`).
//! Alternative approaches could for example offer a way to address data by its age, filtering out
//! logs which contain only operations older than a specified date.
//!
//! ### Topic association and selection
//!
//! A single author's log is usually associated with one or many topics. Authors can also have
//! multiple logs associated with a topic:
//!
//! ```text
//! Author   I / Log 1 ┐
//! Author  II / Log 2 │ <- Topic A
//! Author III / Log 1 │
//! Author III / Log 2 ┘
//!
//! Author   I / Log 1 ┐
//! Author  II / Log 2 │ <- Topic B
//! Author   I / Log 3 ┘
//! ```
//!
//! **When and how do we associate**
//!
//! Association usually takes place when _creating_ or _ingesting_ operations and is normally _not_
//! encoded in the operation itself. A sync protocol however needs to deliver the topic (either
//! through a topic handshake before or metadata around the operation) to be able to correctly
//! associate the log with the topic when receiving it from a remote node.
//!
//! **Confidential topic handshake**
//!
//! In p2panda we disassociate the topic from any immutable data, such as operations, to reduce
//! potential metadata leakage, and on top of that, we aim to encrypt messages fully, either via
//! transport encryption (TLS 1.3 over QUIC) or using `p2panda-spaces` and only exchange topics
//! [confidentially](https://docs.rs/p2panda-discovery/latest/p2panda_discovery/). See _Metadata &
//! privacy_ for more.
//!
//! **Active topics**
//!
//! A sync session does not need to run over _all_ topics the node has ever used. Often we only want
//! to sync over the current active _topic handles_, especially when resources are limited we don't
//! want to overwhelm the network with talking about everything which might already be outdated and
//! redundant.
//!
//! ```text
//! All known topics ever used by a node:
//!
//!  A
//!  B
//!  C ┐
//!  D │ <- Active topics to sync (subset)
//!  E ┘
//!  F
//! ...
//! ```
//!
//! ### Pruning
//!
//! Logs can be [pruned](p2panda_core::prune) in p2panda by announcing a pruning point after which
//! any node can remove the prefix of the log.
//!
//! ```text
//! <- [8] <- [9] <- [10]
//!     ^
//!  Pruning point
//! ```
//!
//! Usually pruning points are encoded in header extensions and only dealt with on _event
//! processing_ layer, outside the sync protocol. However, some sync protocols might want to include
//! the _pruning frontier_ as part of the state vector to indicate that the other nodes do not need
//! to send outdated operations, even if they still have them from their perspective (because they
//! didn't observe the pruning event yet).
//!
//! ### Metadata & privacy
//!
//! Some connectivity substrates come with less than ideal or almost no privacy, for example sending
//! messages via mDNS or packet radio. Data might not be natively protected by transport encryption
//! or other measures. For multicast or broadcast networks we recommend encrypting the payloads
//! before publishing them.
//!
//! **Multicast transport-encryption**
//!
//! One approach can be to use the `Topic` as a symmetric secret (AEAD), if revocation is required
//! one can utilise [p2panda's key agreement
//! scheme](https://docs.rs/p2panda-spaces/latest/p2panda_spaces/) and export the latest symmetric
//! secret key from there. Encrypting payloads as such will result in some sort of _Multicast
//! "Transport Encryption"_ (encrypting messages for transport towards a group instead of another
//! single entity).
//!
//! **Metadata leakage**
//!
//! Sync protocol implementers should be careful with choosing what information they want to include
//! in publicly readable coordination- or sync messages, such as public keys, user identifiers or
//! topics. Any observer of the network would be able to correlate devices and their interest in
//! certain topics.
//!
//! **Confidential sync**
//!
//! In p2panda we attempt to minimize metadata leakage to almost zero by full encryption of payloads
//! and only establish sync confidentially. In an authenticated transport channel (TLS 1.3 etc.) we
//! can rely on [private equality test](https://docs.rs/p2panda-discovery/latest/p2panda_discovery/)
//! before exchanging data, while in a more multicast setting we can use a _proof of decryption_ (we
//! could successfully decrypt a constant value using a secret key which associates us with the same
//! topic) to learn that the other party has the same interest as us.
//!
//! ### Mitigating spam
//!
//! To isolate spammers from a network we either want to avoid connecting to them or prevent
//! spreading their data in the network if we don't have a notion of _connections_ (radio
//! broadcast). In both cases this can be solved with some sort of _deny list_.
//!
//! This means that we need a form of authenticity guarantee (digital signature) for each message
//! delivered over the network so we can reliably verify the authorship and origin of them.
//!
//! Mitigating spam comes as a trade-off of metadata leakage as we have to introduce some sort of
//! device id (or similar) and signing scheme. Sync protocol designers should try to separate these
//! identifiers from the ones used to identify operation authors.
//!
//! ### Store-and-forward
//!
//! Some network topologies require data to be transmitted through intermediaries if we can't or
//! don't want to establish direct connections. This is especially the case for delay-tolerant mesh
//! networks.
//!
//! ```text
//! Node A wants to send a message m to Node C but can't establish a direct connection. They choose
//! to sent the message via B.
//!
//! [A] -- m --> [B] -- m --> [C]
//!               ^
//!  B needs to store-and-forward m
//! ```
//!
//! In these kinds of networks, nodes need to sometimes sync data they are _not interested_ in,
//! meaning that they don't know the topic of this data or do not actively process it (we sometimes
//! say that there's no "active topic handle").
//!
//! This can be achieved with a _store-and-forward_ approach, while it is up to the implementer to
//! decide how much and how long (ring-buffer, expiry date, ..) and for whom they want to store (per
//! topic, device, access via capability, ..) unknown data.
//!
//! Store-and-forward buffers do not always need to reside next to the node but can also be
//! external, such as USB-sticks or p2panda support nodes which behave more like external _mailbox_
//! servers.
//!
//! Ideally all data is fully encrypted and its shape and contents oblivious to anyone. Usually we
//! can't leverage any "data type tricks", such as log-height sync, in these cases or we need to
//! build hybrids where some information about the base data type (backlink, etc.) is still
//! available in plaintext and readable by everyone.
//!
//! As an example we've implemented a data-type-agnostic, generic store-and-forward solution named
//! [MemoryLAN](https://github.com/p2panda/memorylan) based on a bounded, probabilistic
//! Cuckoo-Filter sync protocol for unknown data. This can be integrated as a base layer for any
//! sync protocol on top of a mesh.
//!
//! ### Bandwidth & message throughput
//!
//! Some transports have limitations in the form of maximum packet size. For example, a radio-based
//! transport may restrict us to packets smaller than 255 bytes and may take several seconds to
//! transmit.
//!
//! In order to deal with these limitations, some higher-level protocols may be required. This could
//! take the form of a negotiation protocol allowing channel capacity to be allocated on a per-topic
//! basis. Another protocol might ensure that sync happens lazily (on demand) to ensure a minimum of
//! unnecessary messages being broadcast. Finally, one could compact the payload size of state
//! vectors with the help of probabilistic filters (ie. bloom-, cuckoo-filters etc.).
//!
//! Transports might require some sort of message framing in case the limits have been reached with
//! one single message, however this is out of scope for a sync protocol and rather a transport
//! concern.
use futures_util::stream;
use futures_util::{StreamExt, future};
pub use p2panda_core::logs::{LogHeights, LogRanges, Logs, compare_logs};
use p2panda_core::{AnyOperation, Hash, LogId, SeqNum, VerifyingKey};
use p2panda_store::logs::LogStore;
use p2panda_store::topics::TopicStore;
#[cfg(feature = "ingest")]
pub use p2panda_stream::ingest::{
    IngestError, IngestResult, OooBuffer, OooResult, ingest_operation, validate_operation,
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
