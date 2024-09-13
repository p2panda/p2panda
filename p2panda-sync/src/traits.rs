// SPDX-License-Identifier: AGPL-3.0-or-later

use std::fmt::Debug;
use std::sync::Arc;

use async_trait::async_trait;
use futures::{AsyncRead, AsyncWrite, Sink};

use crate::{SyncError, TopicId};

#[derive(PartialEq, Debug)]
pub enum AppMessage {
    Topic(TopicId),
    Bytes(Vec<u8>),
}

#[async_trait]
pub trait SyncProtocol: Send + Sync + Debug {
    fn name(&self) -> &'static str;

    async fn open(
        self: Arc<Self>,
        topic: &TopicId,
        tx: Box<dyn AsyncWrite + Send + Unpin>,
        rx: Box<dyn AsyncRead + Send + Unpin>,
        app_tx: Box<dyn Sink<AppMessage, Error = SyncError> + Send + Unpin>,
    ) -> Result<(), SyncError>;

    async fn accept(
        self: Arc<Self>,
        tx: Box<dyn AsyncWrite + Send + Unpin>,
        rx: Box<dyn AsyncRead + Send + Unpin>,
        app_tx: Box<dyn Sink<AppMessage, Error = SyncError> + Send + Unpin>,
    ) -> Result<(), SyncError>;
}
