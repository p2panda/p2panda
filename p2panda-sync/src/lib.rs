// SPDX-License-Identifier: AGPL-3.0-or-later

//! Data- and transport-agnostic interface to implement custom sync protocols, compatible with
//! `p2panda-net` or other peer-to-peer networking solutions.
//!
//! Sync or "synchronisation" protocols (also known as "replication protocols") are used to
//! efficiently exchange data between peers.
//!
//! Unlike gossip protocols, sync protocols are better solutions to "catch up on past state". Peers
//! can negotiate scope and access in a sync protocol for any type of data the remote peer
//! currently knows about.
//!
//! While `p2panda-sync` is merely a generic definition of the `Sync` trait interface, compatible
//! with all sorts of data types, it also comes with optional implementations, optimized for
//! efficient sync over append-only log-based data types and helpers to encode wire messages in
//! CBOR.
#[cfg(feature = "cbor")]
pub mod cbor;
#[cfg(feature = "log-sync")]
pub mod log_sync;

use std::fmt::Debug;
use std::hash::Hash;
use std::sync::Arc;

use async_trait::async_trait;
use futures::{AsyncRead, AsyncWrite, Sink};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Data- and transport-agnostic interface to implement a custom sync protocol, compatible with
/// `p2panda-net` or other peer-to-peer networking solutions.
///
/// Implementing a `SyncProtocol` trait needs extra care but is only required when designing custom
/// low-level peer-to-peer protocols and data types. p2panda comes already with solutions which can
/// be used "out of the box", providing implementations for most applications and usecases.
///
/// ## Design
///
/// Sync sessions are designed as a two-party protocol, where an "initiator" starts the session
/// over a "topic" and an "acceptor" learning about the topic to finally exchange the actual data
/// of interest with each other.
///
/// Each protocol is usually following two phases: A "Handshake" phase, used to exchange the
/// "topic" and access control and a "Sync" phase where the requested application data is exchanged
/// and validated.
///
/// ## Privacy and Security
///
/// The `SyncProtocol` trait has been designed to allow privacy-respecting implementations where
/// data (via access control) and the topic itself (for example via Diffie Hellmann) is securely
/// exchanged without revealing any information unnecessarily. This usually takes place inside the
/// "Handshake" phase of the regarding protocol.
///
/// The underlying transport layer should provide automatic authentication of the remote peer, a
/// reliable connection and transport encryption, as in `p2panda-net`.
///
/// ## Streams
///
/// Three distinct data channels are provided by the underlying transport layer to each
/// `SyncProtocol` implementation: `tx` for sending data to the remote peer, `rx` to receive data
/// from the remote peer and `app_tx` to send received data to the higher-level application,
/// validation and persistance layers.
///
/// ## Topics
///
/// Topics are generic data types which can be used to express the interest in a particular subset
/// of the data we want to sync over, for example chat group identifiers or very specific "search
/// queries" for example "give me all documents containing the word 'billy'."
///
/// With the help of the `TopicMap` trait we can keep sync implementations agnostic to specific
/// topic implementations. The sync protocol only needs to feed the "topic" into the "map" which
/// will answer with the actual to-be-synced data entities (for example coming from a store). This
/// allows application developers to use your `SyncProtocol` implementation for their custom
/// `Topic` requirements.
///
/// ## Validation
///
/// Basic data-format and -encoding validation usually takes place during the "Sync" phase of the
/// protocol.
///
/// Further validation which might require more knowledge of the application state or can only be
/// applied after decrypting the payload should be handled _outside_ the sync protocol, by sending
/// it upstream to higher application layers.
///
/// ## Errors
///
/// Protocol implementations operate on multiple layers at the same time, expressed in distinct
/// error categories:
///
/// 1. Unexpected behaviour of the remote peer not following the implemented protocol
/// 2. Handling (rare) critical system failures
#[async_trait]
pub trait SyncProtocol<T, 'a>
where
    Self: Send + Sync + Debug,
    T: Topic,
{
    /// Custom identifier for this sync protocol implementation.
    ///
    /// This is currently only used for debugging or logging purposes.
    fn name(&self) -> &'static str;

    /// Initiate a sync protocol session over the provided bi-directional stream for the given
    /// topic.
    ///
    /// During the "Handshake" phase the "initiator" usually requests access and informs the remote
    /// peer about the "topic" they are interested in exchanging. Implementations for `p2panda-net`
    /// are required to send a `SyncFrom::HandshakeSuccess` message to the application layer (via
    /// `app_tx`) during this phase to inform the backend that we've successfully requested access,
    /// exchanged the topic with the remote peer and are about to begin sync.
    ///
    /// Afterwards it enters the "Sync" phase where the actual application data is exchanged with
    /// the remote peer. If the protocol exchanges data in both directions or not is up to the
    /// regarding implementation. Synced data is forwarded to the application layers via the
    /// `SyncFrom::Data` message (via `app_tx`).
    ///
    /// In case of a detected failure (either through an critical error on our end or an unexpected
    /// behaviour from the remote peer) a `SyncError` is returned.
    async fn initiate(
        self: Arc<Self>,
        topic: T,
        tx: Box<&'a mut (dyn AsyncWrite + Send + Unpin)>,
        rx: Box<&'a mut (dyn AsyncRead + Send + Unpin)>,
        app_tx: Box<&'a mut (dyn Sink<FromSync<T>, Error = SyncError> + Send + Unpin)>,
    ) -> Result<(), SyncError>;

    /// Accept a sync protocol session over the provided bi-directional stream.
    ///
    /// During the "Handshake" phase the "acceptor" usually responds to the access request and
    /// informs the learns about the "topic" from the remote peer they are interested in
    /// exchanging. Implementations for `p2panda-net` are required to send a
    /// `SyncFrom::HandshakeSuccess` message to the application layer (via `app_tx`) during this
    /// phase to inform the backend that we've successfully learned the topic with the remote peer
    /// and are about to begin sync.
    ///
    /// Afterwards it enters the "Sync" phase where the actual application data is exchanged with
    /// the remote peer. If the protocol exchanges data in both directions or not is up to the
    /// regarding implementation. Synced data is forwarded to the application layers via the
    /// `SyncFrom::Data` message (via `app_tx`).
    ///
    /// In case of a detected failure (either through an critical error on our end or an unexpected
    /// behaviour from the remote peer) a `SyncError` is returned.
    async fn accept(
        self: Arc<Self>,
        tx: Box<&'a mut (dyn AsyncWrite + Send + Unpin)>,
        rx: Box<&'a mut (dyn AsyncRead + Send + Unpin)>,
        app_tx: Box<&'a mut (dyn Sink<FromSync<T>, Error = SyncError> + Send + Unpin)>,
    ) -> Result<(), SyncError>;
}

/// Messages which can be sent to the higher application layers for further validation or
/// persistance and the underlying transport layer for managing the sync session.
#[derive(PartialEq, Debug)]
pub enum FromSync<T>
where
    T: Topic,
{
    /// During the "Handshake" phase both peers usually manage access control and negotiate the
    /// "topic" they want to exchange over. This messages indicates that this phase has ended.
    ///
    /// Implementations for `p2panda-net` are required to send this message to the underlying
    /// transport layer to inform the "backend" that we've successfully requested access, exchanged
    /// the topic with the remote peer and are about to begin sync.
    ///
    /// With this information backends can optionally apply optimizations, which might for example
    /// be required to keep application messages in-order (as there might be other channels the
    /// backend might exchange similar data over at the same time).
    HandshakeSuccess(T),

    /// Application data we've received during the sync session from the remote peer and we want to
    /// forward to higher application layers.
    ///
    /// These "frontends" might further process, decrypt payloads, sort messages or apply more
    /// validation before they get finally persisted or rendered to the user. At this point the
    /// sync protocol is merely "forwarding" it without any more knowledge how the data is used.
    ///
    /// Some data-types might be designed with "off-chain" use in mind, where a "header" is crucial
    /// for integrity and authenticity but the actual payload is optional or requested lazily in a
    /// later process. This is why the main data can be expressed as bytes but the second field is
    /// optional. Implementations without this distinction will leave the second field always
    /// `None`.
    Data(Vec<u8>, Option<Vec<u8>>),
}

/// Errors which can occur during sync sessions.
///
/// 1. Critical system failures (bug in p2panda code or sync implementation, sync implementation
///    did not follow "2. Phase Flow" requirements, lack of system resources, etc.)
/// 2. Unexpected Behaviour (remote peer abruptly disconnected, error which got correctly handled
///    in sync implementation, etc.)
#[derive(Debug, Error, PartialEq)]
pub enum SyncError {
    /// Error due to unexpected (buggy or malicious) behaviour of the remote peer.
    ///
    /// Indicates that the sync protocol was not correctly followed, for example due to unexpected
    /// or missing messages, etc.
    ///
    /// Can be used to re-attempt syncing with this peer or down-grading it in priority,
    /// potentially deny-listing if communication failed too often.
    #[error("sync session failed due to unexpected protocol behaviour of remote peer: {0}")]
    UnexpectedBehaviour(String),

    /// Error due to invalid encoding of a message sent by remote peer.
    ///
    /// Note that this error is intended for receiving messages from _remote_ peers which we can't
    /// decode properly. If we fail with encoding our _own_ messages we should rather consider this
    /// an `Critical` error type, as it likely means that there's a buggy implementation.
    #[error("sync session failed due to invalid encoding of message sent by remote peer: {0}")]
    InvalidEncoding(String),

    /// Critical error due to system failure on our end.
    ///
    /// This indicates that our system is running out of resources (storage layer failure etc.) or
    /// we have a buggy implementation.
    #[error("sync session failed due critical system error: {0}")]
    Critical(String),
}

/// Converts critical I/O error (which occurs during codec stream handling) into [`SyncError`].
///
/// This is usually a critical system failure indicating an implementation bug or lacking resources
/// on the user's machine.
///
/// See `Encoder` or `Decoder` `Error` trait type in tokio's codec for more information:
/// https://docs.rs/tokio-util/latest/tokio_util/codec/trait.Decoder.html#associatedtype.Error
impl From<std::io::Error> for SyncError {
    fn from(err: std::io::Error) -> Self {
        match err.kind() {
            // Broken pipes usually indicate that the remote peer closed the connection
            // unexpectedly, this is why we're not treating it as a critical error but as
            // "unexpected behaviour" instead.
            std::io::ErrorKind::BrokenPipe => Self::UnexpectedBehaviour("broken pipe".into()),
            _ => Self::Critical(format!("internal i/o stream error {err}")),
        }
    }
}

/// Identify the particular data-set a peer is interested in syncing.
///
/// Exactly how this is expressed is left up to the user to decide. During sync the "initiator"
/// sends their topic to a remote peer where it is be mapped to their local data-set. Additionally
/// access-control checks can be performed. Once this "handshake" is complete both peers will
/// proceed with the designated sync protocol.
///
/// ## `TopicId` vs `Topic`
///
/// While `TopicId` is merely a 32-byte identifier which can't hold much information other than
/// being a distinct identifier of a single data item or subset of them, we can use `Topic` to
/// implement custom data types which can "query" very specific data items. Peers can for example
/// announce that they'd like "all events from the 27th of September 23 until today" with `Topic`.
///
/// Consult the `TopicId` documentation in `p2panda-net` for more information.
pub trait Topic:
    Clone + Debug + Eq + Hash + Send + Sync + Serialize + for<'a> Deserialize<'a>
{
}

/// Maps a `Topic` to the related data being sent over the wire during sync.
///
/// Each `SyncProtocol` implementation defines the type of data it is expecting to sync and how the
/// scope for a particular session should be identified. Sync protocol users can provide an
/// implementation of `TopicMap` so that scope `S` for data can be retrieved for a specific topic
/// `T` when a peer initiates or accepts a sync session.
///
/// Since `TopicMap` is generic we can use the same mapping across different sync implementations
/// for the same data type when necessary.
///
/// ## Designing `TopicMap` for applications
///
/// Considering an example chat application which is based on append-only log data types, we
/// probably want to organise messages from an author for a certain chat group into one log each.
/// Like this, a chat group can be expressed as a collection of one to potentially many logs (one
/// per member of the group):
///
/// ```text
/// All authors: A, B and C
/// All chat groups: 1 and 2
///
/// "Chat group 1 with members A and B"
/// - Log A1
/// - Log B1
///
/// "Chat group 2 with members A, B and C"
/// - Log A2
/// - Log B2
/// - Log C2
/// ```
///
/// If we implement `Topic` to express that we're interested in syncing over a specific chat group,
/// for example "Chat Group 2" we would implement `TopicMap` to give us all append-only logs of all
/// members inside this group, that is the entries inside logs `A2`, `B2` and `C2`.
#[async_trait]
pub trait TopicMap<T, S>: Debug + Send + Sync
where
    T: Topic,
{
    async fn get(&self, topic: &T) -> Option<S>;
}
