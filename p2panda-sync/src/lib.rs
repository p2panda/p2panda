// SPDX-License-Identifier: AGPL-3.0-or-later

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

/// Identify the particular data-set a peer is interested in syncing.
///
/// Exactly how this is expressed is left up to the user to decide. During sync the "initiator"
/// sends their topic to a remote peer where it is be mapped to their local data-set and
/// access-control checks can be performed. Once this "handshake" is complete both peers will
/// proceed with the designated sync protocol.
pub trait Topic:
    Clone + Debug + Eq + Hash + Send + Sync + Serialize + for<'a> Deserialize<'a>
{
}

/// Maps a topic to the related data being sent over the wire during sync.
///
/// Each `SyncProtocol` implementation defines the type of data it is expecting to sync and how the
/// scope for a particular session should be identified. Sync protocol users can provide an
/// implementation of `TopicMap` so that a scope `S` can be retrieved for a specific topic `T` when
/// a peer initiates or accepts a sync session.
#[async_trait]
pub trait TopicMap<T, S>: Debug + Send + Sync
where
    T: Topic,
{
    async fn get(&self, topic: &T) -> Option<S>;
}

#[async_trait]
pub trait SyncProtocol<T, 'a>
where
    Self: Send + Sync + Debug,
    T: Topic,
{
    fn name(&self) -> &'static str;

    async fn initiate(
        self: Arc<Self>,
        topic: T,
        tx: Box<&'a mut (dyn AsyncWrite + Send + Unpin)>,
        rx: Box<&'a mut (dyn AsyncRead + Send + Unpin)>,
        app_tx: Box<&'a mut (dyn Sink<FromSync<T>, Error = SyncError> + Send + Unpin)>,
    ) -> Result<(), SyncError>;

    async fn accept(
        self: Arc<Self>,
        tx: Box<&'a mut (dyn AsyncWrite + Send + Unpin)>,
        rx: Box<&'a mut (dyn AsyncRead + Send + Unpin)>,
        app_tx: Box<&'a mut (dyn Sink<FromSync<T>, Error = SyncError> + Send + Unpin)>,
    ) -> Result<(), SyncError>;
}

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
            // unexpectedly, this is why we're not treating it as an critical error buy as
            // "unexpected behaviour" instead.
            std::io::ErrorKind::BrokenPipe => Self::UnexpectedBehaviour("broken pipe".into()),
            _ => Self::Critical(format!("internal i/o stream error {err}")),
        }
    }
}

#[derive(PartialEq, Debug)]
pub enum FromSync<T>
where
    T: Topic,
{
    HandshakeSuccess(T),
    Data(Vec<u8>, Option<Vec<u8>>),
}
