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

#[async_trait]
pub trait SyncProtocol<'a>: Send + Sync + Debug {
    fn name(&self) -> &'static str;

    async fn open(
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
    /// Error which can occur in a running sync session
    #[error("sync protocol error: {0}")]
    Protocol(String),

    /// I/O error which occurs during stream handling
    #[error("input/output error: {0}")]
    IoError(#[from] std::io::Error),

    /// Error which occurs when encoding or decoding protocol messages
    #[error("codec error: {0}")]
    Codec(String),

    /// Custom error to handle other cases
    #[error("custom error: {0}")]
    Custom(String),
}

#[derive(PartialEq, Debug)]
pub enum FromSync {
    Topic(TopicId),
    Data(Vec<u8>, Option<Vec<u8>>),
}
