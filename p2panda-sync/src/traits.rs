// SPDX-License-Identifier: AGPL-3.0-or-later

use std::sync::Arc;

use async_trait::async_trait;
use futures::{AsyncRead, AsyncWrite};

use crate::SyncError;

#[async_trait]
pub trait SyncProtocol: Send + Sync {
    fn name(&self) -> &'static str;

    async fn run(
        self: Arc<Self>,
        tx: Box<dyn AsyncWrite + Send + Unpin>,
        rx: Box<dyn AsyncRead + Send + Unpin>,
    ) -> Result<(), SyncError>;
}
