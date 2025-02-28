// SPDX-License-Identifier: MIT OR Apache-2.0

#![cfg_attr(doctest, doc=include_str!("../README.md"))]

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
//! In addition to the generic definition of the `SyncProtocol` trait, `p2panda-sync` includes
//! optional implementations for efficient sync of append-only log-based data types. These optional
//! implementations may be activated via feature flags. Finally, `p2panda-sync` provides helpers to
//! encode wire messages in CBOR.
#[cfg(feature = "cbor")]
pub mod cbor;
#[cfg(feature = "log-sync")]
pub mod log_sync;
#[cfg(feature = "test-protocols")]
pub mod test_protocols;

use std::fmt::Debug;
use std::hash::Hash;
use std::sync::Arc;

use async_trait::async_trait;
use futures::{AsyncRead, AsyncWrite, Sink};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Traits to implement a custom sync protocol.
///
/// Implementing a `SyncProtocol` trait needs extra care and is only required when designing custom
/// low-level peer-to-peer protocols and data types. p2panda already comes with solutions which can
/// be used "out of the box", providing implementations for most applications and usecases.
///
/// ## Design
///
/// Sync sessions take place when two peers connect to each other and follow the sync protocol.
/// They are designed as a two-party protocol featuring an "initiator" and an "acceptor" role.
///
/// Each protocol usually follows two phases: 1) The "Handshake" phase, during which the
/// "initiator" sends the "topic query" and any access control data to the "acceptor", and 2) The
/// "Sync" phase, where the requested application data is finally exchanged and validated.
///
/// ## Privacy and Security
///
/// The `SyncProtocol` trait has been designed to allow privacy-respecting implementations where
/// application data (via access control) and the topic query itself (for example via Diffie
/// Hellmann) is securely exchanged without revealing any information to unknown peers
/// unnecessarily. This usually takes place during the "Handshake" phase of the protocol.
///
/// The underlying transport layer should provide automatic authentication of the remote peer, a
/// reliable connection and transport encryption. `p2panda-net`, for example, uses self-certified
/// TLS 1.3 over QUIC.
///
/// ## Streams
///
/// Three distinct data channels are provided by the underlying transport layer to each
/// `SyncProtocol` implementation: `tx` for sending data to the remote peer, `rx` to receive data
/// from the remote peer and `app_tx` to send received data to the higher-level application-,
/// validation- and persistance-layers.
///
/// ## Topic queries
///
/// Topics queries are generic data types which can be used to subjectively express interest in a
/// particular subset of the data we want to sync over, like chat group identifiers or very
/// specific "search queries", for example "give me all documents containing the word 'billy'."
///
/// With the help of the `TopicMap` trait we can keep sync implementations agnostic to specific
/// topic query implementations. The sync protocol only needs to feed the "topic query" into the
/// "map" which will answer with the actual to-be-synced data entities (for example coming from a
/// store). This allows application developers to re-use your `SyncProtocol` implementation for
/// their custom `TopicQuery` requirements.
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
    T: TopicQuery,
{
    /// Custom identifier for this sync protocol implementation.
    ///
    /// This is currently only used for debugging or logging purposes.
    fn name(&self) -> &'static str;

    /// Initiate a sync protocol session over the provided bi-directional stream for the given
    /// topic query.
    ///
    /// During the "Handshake" phase the "initiator" usually requests access and informs the remote
    /// peer about the "topic query" they are interested in. Implementations for `p2panda-net`
    /// are required to send a `SyncFrom::HandshakeSuccess` message to the application layer (via
    /// `app_tx`) during this phase to inform the backend that we've successfully requested access,
    /// exchanged the topic query with the remote peer and are about to begin sync.
    ///
    /// After the "Handshake" is complete the protocol enters the "Sync" phase, during which
    /// the actual application data is exchanged with the remote peer. It's left up to each
    /// protocol implementation to decide whether data is exchanged in one or both directions.
    /// Synced data is forwarded to the application layers via the `SyncFrom::Data` message
    /// (via `app_tx`).
    ///
    /// In case of a detected failure (either through a critical error on our end or an unexpected
    /// behaviour from the remote peer) a `SyncError` is returned.
    async fn initiate(
        self: Arc<Self>,
        topic_query: T,
        tx: Box<&'a mut (dyn AsyncWrite + Send + Unpin)>,
        rx: Box<&'a mut (dyn AsyncRead + Send + Unpin)>,
        app_tx: Box<&'a mut (dyn Sink<FromSync<T>, Error = SyncError> + Send + Unpin)>,
    ) -> Result<(), SyncError>;

    /// Accept a sync protocol session over the provided bi-directional stream.
    ///
    /// During the "Handshake" phase the "acceptor" usually responds to the access request and
    /// learns about the "topic query" from the remote peer. Implementations for `p2panda-net` are
    /// required to send a `SyncFrom::HandshakeSuccess` message to the application layer (via
    /// `app_tx`) during this phase to inform the backend that the topic query has been
    /// successfully received from the remote peer and that data exchange is about to begin.
    ///
    /// After the "Handshake" is complete the protocol enters the "Sync" phase, during which
    /// the actual application data is exchanged with the remote peer. It's left up to each
    /// protocol implementation to decide whether data is exchanged in one or both directions.
    /// Synced data is forwarded to the application layers via the `SyncFrom::Data` message
    /// (via `app_tx`).
    ///
    /// In case of a detected failure (either through a critical error on our end or an unexpected
    /// behaviour from the remote peer) a `SyncError` is returned.
    async fn accept(
        self: Arc<Self>,
        tx: Box<&'a mut (dyn AsyncWrite + Send + Unpin)>,
        rx: Box<&'a mut (dyn AsyncRead + Send + Unpin)>,
        app_tx: Box<&'a mut (dyn Sink<FromSync<T>, Error = SyncError> + Send + Unpin)>,
    ) -> Result<(), SyncError>;
}

/// Messages which can be sent to the higher application layers (for further validation or
/// persistance) and the underlying transport layer (for managing the sync session).
#[derive(Debug, PartialEq)]
pub enum FromSync<T>
where
    T: TopicQuery,
{
    /// During the "Handshake" phase both peers usually manage access control and negotiate the
    /// "topic query" they want to exchange over. This message indicates that this phase has ended.
    ///
    /// Implementations for `p2panda-net` are required to send this message to the underlying
    /// transport layer to inform the "backend" that we've successfully requested access, exchanged
    /// the topic query with the remote peer and are about to begin sync.
    ///
    /// With this information backends can optionally apply optimisations, which might for example
    /// be required to keep application messages in-order (as there might exist other channels the
    /// backend exchanges similar data over at the same time).
    HandshakeSuccess(T),

    /// Application data we've received during the sync session from the remote peer and want to
    /// forward to higher application layers.
    ///
    /// These "frontends" might further process, decrypt payloads, sort messages or apply more
    /// validation before they get finally persisted or rendered to the user. At this point the
    /// sync protocol is merely "forwarding" it without any knowledge of how the data is used.
    Data {
        /// Exchanged data from sync session.
        ///
        /// Some data-types might be designed with "off-chain" use in mind, where a "header" is
        /// crucial for integrity and authenticity but the actual "payload" is optional or
        /// requested lazily in a later process.
        header: Vec<u8>,

        /// Optional "body" which can represent "off-chain" application data.
        ///
        /// This is useful for realising "off-chain" compatible data types. Implementations without
        /// this distinction will always leave this field as `None` and only encode their data
        /// types in the `header` field.
        payload: Option<Vec<u8>>,
    },
}

/// Errors which can occur during sync sessions.
///
/// 1. Critical system failures (ie. bug in p2panda code or sync implementation, sync
///    implementation did not follow "2. Phase Flow" requirements, lack of system resources, etc.)
/// 2. Unexpected Behaviour (ie. remote peer abruptly disconnected, error which got correctly
///    caught in sync implementation, etc.)
#[derive(Debug, PartialEq, Error)]
pub enum SyncError {
    /// Error due to unexpected (buggy or malicious) behaviour of the remote peer.
    ///
    /// Indicates that the sync protocol was not correctly followed, for example due to unexpected
    /// or missing messages, etc.
    ///
    /// Can be used by a backend to re-attempt syncing with this peer or down-grading it in
    /// priority, potentially deny-listing if communication failed too often.
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
/// <https://docs.rs/tokio-util/latest/tokio_util/codec/trait.Decoder.html#associatedtype.Error>
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

/// Identify the particular dataset a peer is interested in syncing.
///
/// Exactly how this is expressed is left up to the user to decide. During sync the "initiator"
/// sends their topic query to a remote peer where it is be mapped to their local dataset.
/// Additional access-control checks can be performed. Once this "handshake" is complete both
/// peers will proceed with the designated sync protocol.
///
/// ## `TopicId` vs `TopicQuery`
///
/// While `TopicId` is merely a 32-byte identifier which can't hold much information other than
/// being a distinct identifier of a single data item or collection of them, we can use `TopicQuery` to
/// implement custom data types representing "queries" for very specific data items. Peers can for
/// example announce that they'd like "all events from the 27th of September 23 until today" with
/// `TopicQuery`.
///
/// Consult the `TopicId` documentation in `p2panda-net` for more information.
pub trait TopicQuery:
    // Data types implementing `TopicQuery` also need to implement `Eq` and `Hash` in order to allow 
    // backends to organise sync sessions per topic query and peer, along with `Serialize` and 
    // `Deserialize` to allow sending topics over the wire.
    Clone + Debug + Eq + Hash + Send + Sync + Serialize + for<'a> Deserialize<'a>
{
}
