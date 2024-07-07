// SPDX-License-Identifier: AGPL-3.0-or-later

use futures_core::future::LocalBoxFuture;
use p2panda_core::{Extension, Operation};

use crate::context::Context;
use crate::ingest::{Ingest, IngestResult};
use crate::layer::Layer;

pub struct BoxedMiddleware<E>(pub(crate) Box<dyn BoxedMiddlewareInner<E>>);

impl<E> BoxedMiddleware<E> {
    pub fn new<R>(inner: R) -> Self
    where
        R: Ingest<E> + Clone + 'static,
        E: Extension + 'static,
    {
        Self(Box::new(inner))
    }
}

impl<E> Layer<BoxedMiddleware<E>> for BoxedMiddleware<E> {
    type Middleware = BoxedMiddleware<E>;

    fn layer(&self, _inner: BoxedMiddleware<E>) -> Self::Middleware {
        todo!()
    }
}

impl<E> Clone for BoxedMiddleware<E>
where
    E: Extension,
{
    fn clone(&self) -> Self {
        Self(self.0.clone_box())
    }
}

pub trait BoxedMiddlewareInner<E>
where
    E: Extension,
{
    fn ingest(
        &mut self,
        context: Context,
        operation: Operation<E>,
    ) -> LocalBoxFuture<IngestResult<E>>;

    fn clone_box(&self) -> Box<dyn BoxedMiddlewareInner<E>>;
}

impl<T, E> BoxedMiddlewareInner<E> for T
where
    T: Ingest<E> + Clone + 'static,
    E: Extension + 'static,
{
    fn ingest(
        &mut self,
        context: Context,
        operation: Operation<E>,
    ) -> LocalBoxFuture<IngestResult<E>> {
        Box::pin(Ingest::ingest(self, context, operation))
    }

    fn clone_box(&self) -> Box<dyn BoxedMiddlewareInner<E>> {
        Box::new(self.clone())
    }
}
