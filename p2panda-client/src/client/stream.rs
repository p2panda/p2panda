// SPDX-License-Identifier: MIT OR Apache-2.0

use std::marker::PhantomData;
use std::pin::Pin;
use std::task::{Context, Poll};

use futures_core::Stream;
use p2panda_core::Hash;
use p2panda_core::cbor::{EncodeError, encode_cbor};
use thiserror::Error;

use crate::client::message::Message;
use crate::connector::{Connector, StreamEvent};
use crate::controller::{Consumer, ConsumerError, Controller, ControllerError};
use crate::{Checkpoint, Subject};

pub struct StreamHandle<M, C>
where
    C: Connector,
{
    subject: Subject,
    controller: Controller<C>,
    _marker: PhantomData<M>,
}

impl<M, C> StreamHandle<M, C>
where
    M: Message,
    C: Connector,
{
    pub(crate) fn new(subject: Subject, controller: Controller<C>) -> Self {
        Self {
            subject,
            controller,
            _marker: PhantomData,
        }
    }

    pub async fn publish(&self, message: M) -> Result<Hash, StreamError<C>> {
        // @TODO: Use operation store here to properly forge a header.
        let header_bytes = vec![];

        // @TODO: Is it okay to be opiniated on the payload encoding at this layer?
        let body_bytes = encode_cbor(&message)?;

        self.controller
            .publish(self.subject.clone(), header_bytes, body_bytes)
            .await
            .map_err(StreamError::Controller)
    }

    pub async fn subscribe(&self) -> Result<StreamSubscription<M, C>, StreamError<C>> {
        let live = true;
        let consumer = self
            .controller
            .subscribe(self.subject.clone(), live)
            .await
            .map_err(StreamError::Controller)?;
        Ok(StreamSubscription::new(consumer))
    }

    pub async fn commit(&self, operation_id: Hash) -> Result<(), StreamError<C>> {
        self.controller
            .commit(operation_id)
            .await
            .map_err(StreamError::Controller)
    }
}

pub struct StreamSubscription<M, C>
where
    C: Connector,
{
    consumer: Consumer<C>,
    _marker: PhantomData<M>,
}

impl<M, C> StreamSubscription<M, C>
where
    M: Message,
    C: Connector,
{
    fn new(consumer: Consumer<C>) -> Self {
        Self {
            consumer,
            _marker: PhantomData,
        }
    }

    pub async fn commit(&mut self, operation_id: Hash) -> Result<(), StreamError<C>> {
        self.consumer.commit(operation_id).await?;
        Ok(())
    }

    pub async fn replay(&mut self, from: Checkpoint) -> Result<(), StreamError<C>> {
        self.consumer.replay(from).await?;
        Ok(())
    }

    pub async fn unsubscribe(&mut self) -> Result<(), StreamError<C>> {
        self.consumer.unsubscribe().await?;
        Ok(())
    }
}

impl<M, C> Stream for StreamSubscription<M, C>
where
    M: Message + Unpin,
    C: Connector,
{
    type Item = Result<StreamEvent, StreamError<C>>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();
        Pin::new(&mut this.consumer)
            .poll_next(cx)
            .map(|opt| opt.map(|result| result.map_err(StreamError::Consumer)))
    }
}

#[derive(Debug, Error)]
pub enum StreamError<C>
where
    C: Connector,
{
    #[error(transparent)]
    Encode(#[from] EncodeError),

    #[error(transparent)]
    Controller(#[from] ControllerError<C>),

    #[error(transparent)]
    Consumer(#[from] ConsumerError<C>),
}
