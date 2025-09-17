// SPDX-License-Identifier: MIT OR Apache-2.0

use std::marker::PhantomData;
use std::pin::Pin;
use std::task::{Context, Poll};

use futures_core::Stream;
use p2panda_core::Hash;
use p2panda_core::cbor::{EncodeError, encode_cbor};
use thiserror::Error;

use crate::backend::{Backend, StreamEvent};
use crate::client::message::Message;
use crate::controller::{Consumer, Controller, ControllerError};
use crate::{Checkpoint, Subject};

pub struct StreamHandle<M, B>
where
    B: Backend,
{
    subject: Subject,
    controller: Controller<B>,
    _marker: PhantomData<M>,
}

impl<M, B> StreamHandle<M, B>
where
    M: Message,
    B: Backend,
{
    pub(crate) fn new(subject: Subject, controller: Controller<B>) -> Self {
        Self {
            subject,
            controller,
            _marker: PhantomData,
        }
    }

    pub async fn publish(&self, message: M) -> Result<Hash, StreamError<B>> {
        // @TODO: Use operation store here to properly forge a header.
        let header_bytes = vec![];

        // @TODO: Is it okay to be opiniated on the payload encoding at this layer?
        let body_bytes = encode_cbor(&message)?;

        self.controller
            .publish(self.subject.clone(), header_bytes, body_bytes)
            .await
            .map_err(StreamError::Controller)
    }

    pub async fn subscribe(&self) -> Result<StreamSubscription<M, B>, StreamError<B>> {
        let live = true;

        let consumer = self
            .controller
            .subscribe(self.subject.clone(), live)
            .await
            .map_err(StreamError::Controller)?;

        Ok(StreamSubscription::new(consumer))
    }

    pub async fn commit(&self, operation_id: Hash) -> Result<(), StreamError<B>> {
        self.controller
            .commit(operation_id)
            .await
            .map_err(StreamError::Controller)
    }
}

pub struct StreamSubscription<M, B>
where
    B: Backend,
{
    consumer: Consumer<B>,
    _marker: PhantomData<M>,
}

impl<M, B> StreamSubscription<M, B>
where
    M: Message,
    B: Backend,
{
    fn new(consumer: Consumer<B>) -> Self {
        Self {
            consumer,
            _marker: PhantomData,
        }
    }

    pub async fn commit(&mut self, operation_id: Hash) -> Result<(), StreamError<B>> {
        self.consumer
            .commit(operation_id)
            .await
            .map_err(StreamError::Consumer)
    }

    pub async fn replay(&mut self, from: Checkpoint) -> Result<(), StreamError<B>> {
        self.consumer
            .replay(from)
            .await
            .map_err(StreamError::Consumer)
    }

    pub async fn unsubscribe(&mut self) -> Result<(), StreamError<B>> {
        self.consumer
            .unsubscribe()
            .await
            .map_err(StreamError::Consumer)
    }
}

impl<M, B> Stream for StreamSubscription<M, B>
where
    M: Message + Unpin,
    B: Backend,
{
    type Item = Result<StreamEvent, StreamError<B>>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        Pin::new(&mut this.consumer)
            .poll_next(cx)
            .map(|opt| opt.map(|result| result.map_err(StreamError::Consumer)))
    }
}

#[derive(Debug, Error)]
pub enum StreamError<B>
where
    B: Backend,
{
    #[error(transparent)]
    Encode(#[from] EncodeError),

    #[error(transparent)]
    Controller(#[from] ControllerError<B>),

    #[error(transparent)]
    Consumer(crate::controller::ConsumerError<B>),
}
