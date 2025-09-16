// SPDX-License-Identifier: MIT OR Apache-2.0

use std::pin::Pin;
use std::task::{Context, Poll};

use futures_core::Stream;
use p2panda_core::Hash;
use thiserror::Error;

use crate::Subject;
use crate::backend::{Backend, StreamEvent, Subscription, SubscriptionId};
use crate::controller::{Controller, ControllerError};

pub struct Consumer<B>
where
    B: Backend,
{
    subject: Subject,
    subscription_id: SubscriptionId,
    controller: Controller<B>,
    event_stream: <B::Subscription as Subscription>::EventStream,
}

impl<B> Consumer<B>
where
    B: Backend,
{
    pub(crate) fn new(
        subject: Subject,
        subscription_id: SubscriptionId,
        event_stream: <B::Subscription as Subscription>::EventStream,
        controller: Controller<B>,
    ) -> Self {
        Self {
            subject,
            subscription_id,
            controller,
            event_stream,
        }
    }

    pub async fn commit(&mut self, operation_id: Hash) -> Result<(), ConsumerError<B>> {
        self.controller
            .commit(operation_id)
            .await
            .map_err(ConsumerError::Controller)?;
        Ok(())
    }

    // @TODO: Wait for "unsubscribed" event to arrive from backend. Something to handle as an
    // internal state in Stream implementation.
    pub async fn unsubscribe(self) -> Result<(), ConsumerError<B>> {
        self.controller.unsubscribe(self.subscription_id).await?;
        drop(self);
        Ok(())
    }
}

impl<B> Stream for Consumer<B>
where
    B: Backend,
{
    type Item = Result<StreamEvent, ConsumerError<B>>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match Pin::new(&mut self.event_stream).poll_next(cx) {
            Poll::Ready(Some(Ok(event))) => Poll::Ready(Some(Ok(event))),
            Poll::Ready(Some(Err(err))) => Poll::Ready(Some(Err(ConsumerError::Subscription(err)))),
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}

#[derive(Debug, Error)]
pub enum ConsumerError<B>
where
    B: Backend,
{
    #[error(transparent)]
    Controller(#[from] ControllerError<B>),

    #[error("{0}")]
    Subscription(<B::Subscription as Subscription>::Error),
}
