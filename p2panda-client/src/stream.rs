// SPDX-License-Identifier: MIT OR Apache-2.0

use std::convert::Infallible;
use std::marker::PhantomData;
use std::pin::Pin;
use std::task::{Context, Poll};

use futures_core::future::BoxFuture;
use futures_core::stream::BoxStream;
use futures_core::{Stream, ready};
use futures_util::stream::{ForEach, Map, Then};
use futures_util::{FutureExt, StreamExt};
use p2panda_core::{Hash, Header, Operation};
use pin_project::pin_project;
use thiserror::Error;

use crate::controller::{Controller, ControllerError};
use crate::{OperationStream, Query, StreamProcessor, StreamProcessorOutput};

pub struct StreamHandler<B, P, E, M> {
    query: Query,
    controller: Controller<B>,
    processor: P,
    _marker: PhantomData<(E, M)>,
}

impl<B, P, E, M> StreamHandler<B, P, E, M>
where
    B: OperationStream,
    P: StreamProcessor<Input = Operation<E>>,
{
    pub fn new(controller: Controller<B>, processor: P, query: Query) -> Self {
        Self {
            query,
            controller,
            processor,
            _marker: PhantomData,
        }
    }

    pub async fn publish(&self, message: M) -> Result<Hash, StreamError<B>> {
        todo!()
    }

    pub async fn commit(&self, operation_id: Hash) -> Result<Hash, StreamError<B>> {
        todo!()
    }

    pub async fn subscribe(&self) -> Result<StreamSubscription<'_, B, P, E, M>, StreamError<B>>
    where
        E: Send + Sync + 'static,
    {
        let stream = self.controller.subscribe::<E>(self.query.clone()).await?;

        Ok(StreamSubscription {
            stream,
            processor: self.processor.clone(),
            future: None,
            _marker: PhantomData,
        })
    }
}

#[pin_project]
pub struct StreamSubscription<'a, B, P, E, M>
where
    P: StreamProcessor,
{
    #[pin]
    stream: BoxStream<'a, P::Input>,
    processor: P,
    #[pin]
    future: Option<TerribleFut<P>>,
    _marker: PhantomData<(B, E, M)>,
}

type TerribleFut<P> = Pin<
    Box<
        dyn Future<
                Output = Result<
                    StreamProcessorOutput<<P as StreamProcessor>::Output>,
                    <P as StreamProcessor>::Error,
                >,
            > + Send,
    >,
>;

impl<'a, B, P, E, M> Stream for StreamSubscription<'a, B, P, E, M>
where
    P: StreamProcessor,
{
    type Item = Result<P::Output, P::Error>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut this = self.project();

        Poll::Ready(loop {
            if let Some(fut) = this.future.as_mut().as_pin_mut() {
                let item = ready!(fut.poll(cx));
                this.future.set(None);
                match item {
                    Ok(StreamProcessorOutput::Completed(result)) => break Some(Ok(result)),
                    Ok(StreamProcessorOutput::Deferred) => continue,
                    Err(err) => break Some(Err(err)),
                }
            } else if let Some(item) = ready!(this.stream.as_mut().poll_next(cx)) {
                let processor = this.processor.clone();
                let fut = async move { processor.process(item).await };

                this.future.set(Some(Box::pin(fut)));
            } else {
                break None;
            }
        })
    }
}

pub struct StreamEvent<B, E, M> {
    header: Header<E>,
    body: M,
    _marker: PhantomData<B>,
}

impl<B, E, M> StreamEvent<B, E, M>
where
    B: OperationStream,
{
    pub async fn commit(&self) -> Result<(), StreamError<B>> {
        todo!()
    }
}

#[derive(Debug, Error)]
pub enum StreamError<B>
where
    B: OperationStream,
{
    #[error(transparent)]
    Controller(#[from] ControllerError<B>),
}
