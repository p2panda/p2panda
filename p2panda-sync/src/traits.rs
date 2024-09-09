// SPDX-License-Identifier: AGPL-3.0-or-later

use std::sync::Arc;

use async_trait::async_trait;
use futures::{AsyncRead, AsyncWrite, Sink};

use crate::{SyncError, TopicId};

#[async_trait]
pub trait SyncProtocol: Send + Sync {
    fn name(&self) -> &'static str;

    async fn run(
        self: Arc<Self>,
        topic: &TopicId,
        tx: Box<dyn AsyncWrite + Send + Unpin>,
        rx: Box<dyn AsyncRead + Send + Unpin>,
        app_tx: Box<dyn Sink<Vec<u8>, Error = SyncError> + Send + Unpin>,
    ) -> Result<(), SyncError>;
}
