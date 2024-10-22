// SPDX-License-Identifier: AGPL-3.0-or-later

#[cfg(feature = "cbor")]
pub mod cbor;
#[cfg(feature = "log-sync")]
pub mod log_sync;

use std::fmt::Debug;
use std::sync::Arc;

use async_trait::async_trait;
use futures::{AsyncRead, AsyncWrite, Sink};
use thiserror::Error;

pub type TopicId = [u8; 32];

/// Trait used for mapping a generic topic to a single or collection of logs
#[async_trait]
pub trait TopicMap<K, V> {
    async fn get(&self, topic: &K) -> Option<Vec<V>>;
}

#[async_trait]
pub trait SyncProtocol<'a>: Send + Sync + Debug {
    fn name(&self) -> &'static str;

    async fn initiate(
        self: Arc<Self>,
        topic: &TopicId,
        tx: Box<&'a mut (dyn AsyncWrite + Send + Unpin)>,
        rx: Box<&'a mut (dyn AsyncRead + Send + Unpin)>,
        app_tx: Box<&'a mut (dyn Sink<FromSync, Error = SyncError> + Send + Unpin)>,
    ) -> Result<(), SyncError>;

    async fn accept(
        self: Arc<Self>,
        tx: Box<&'a mut (dyn AsyncWrite + Send + Unpin)>,
        rx: Box<&'a mut (dyn AsyncRead + Send + Unpin)>,
        app_tx: Box<&'a mut (dyn Sink<FromSync, Error = SyncError> + Send + Unpin)>,
    ) -> Result<(), SyncError>;
}

#[derive(Error, Debug)]
pub enum SyncError {
    /// Error due to unexpected (buggy or malicious) behaviour of the remote peer.
    ///
    /// Indicates that the sync protocol was not correctly followed, for example due to unexpected
    /// or missing messages, invalid encoding, etc.
    ///
    /// Can be used to re-attempt syncing with this peer or down-grading it in priority,
    /// potentially deny-listing if communication failed too often.
    #[error("sync session failed due to unexpected protocol behaviour of remote peer: {0}")]
    RemoteUnexpectedBehaviour(String),

    /// Remote peer didn't show any activity for some time.
    ///
    /// Can be used to re-attempt syncing with this peer or down-grading it in priority,
    /// potentially deny-listing if communication failed too often.
    #[error("sync session failed due to remote peer timeout")]
    RemoteTimeout,

    /// Critical error due to system failure on our end.
    ///
    /// This indicates that our system is running out of resources (storage layer failure etc.) or
    /// we have a buggy implementation.
    #[error("sync session failed due critical system error: {0}")]
    Critical(String),
}

/// Converts critical I/O error which occurs during stream handling into [`SyncError`].
///
/// This is usually a critical system failure indicating an implementation bug or lacking resources
/// on the user's machine.
impl From<std::io::Error> for SyncError {
    fn from(err: std::io::Error) -> Self {
        Self::Critical(format!("internal i/o stream error {err}"))
    }
}

#[derive(PartialEq, Debug)]
pub enum FromSync {
    Topic(TopicId),
    Data(Vec<u8>, Option<Vec<u8>>),
}
