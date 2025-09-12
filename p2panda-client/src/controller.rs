// SPDX-License-Identifier: MIT OR Apache-2.0

use std::sync::Arc;

use futures_core::stream::BoxStream;
use p2panda_core::{Hash, Operation};
use thiserror::Error;

use crate::ephemeral::EphemeralStreamHandler;
use crate::query::Query;
use crate::stream::{StreamError, StreamEvent, StreamHandler};
use crate::{Checkpoint, EphemeralStream, OperationStream};

pub(crate) struct Controller<B> {
    inner: Arc<Inner<B>>,
}

impl<B> Clone for Controller<B> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

struct Inner<B> {
    backend: B,
}

impl<B> Controller<B>
where
    B: OperationStream,
{
    pub fn new(backend: B) -> Self {
        let inner = Inner { backend };

        Self {
            inner: Arc::new(inner),
        }
    }

    pub async fn subscribe<E>(
        &self,
        query: Query,
    ) -> Result<BoxStream<'_, Operation<E>>, ControllerError<B>>
    where
        E: Send + Sync + 'static,
    {
        // @TODO: Take checkpoint from store to continue from where controller stopped last
        let rx = self
            .inner
            .backend
            .subscribe(query, Checkpoint::default(), true)
            .await
            .map_err(ControllerError::OperationStream)?;
        Ok(rx)
    }
}

#[derive(Debug, Error)]
pub enum ControllerError<B>
where
    B: OperationStream,
{
    #[error("{0}")]
    OperationStream(<B as OperationStream>::Error),
}
