// SPDX-License-Identifier: AGPL-3.0-or-later

use std::fmt::Debug;
use std::sync::Arc;

use async_trait::async_trait;
use futures::{AsyncRead, AsyncWrite, Sink};

use crate::{FromSync, SyncError, TopicId};

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
