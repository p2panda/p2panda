// SPDX-License-Identifier: MIT OR Apache-2.0

use std::pin::Pin;
use std::task::{Context, Poll};

use futures_core::Stream;
use p2panda_core::Hash;
use thiserror::Error;

use crate::backend::{Backend, StreamEvent, Subscription, SubscriptionId};
use crate::controller::{Controller, ControllerError};
use crate::{Checkpoint, Subject};

enum ConsumerState {
    Active,
    Unsubscribing,
    Unsubscribed,
}

pub struct Consumer<B>
where
    B: Backend,
{
    subject: Subject,
    subscription_id: SubscriptionId,
    controller: Controller<B>,
    event_stream: <B::Subscription as Subscription>::EventStream,
    state: ConsumerState,
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
            state: ConsumerState::Active,
        }
    }

    pub async fn commit(&mut self, operation_id: Hash) -> Result<(), ConsumerError<B>> {
        self.controller
            .commit(operation_id)
            .await
            .map_err(ConsumerError::Controller)?;
        Ok(())
    }

    pub async fn replay(&mut self, from: Checkpoint) -> Result<(), ConsumerError<B>> {
        self.controller
            .replay(self.subscription_id, from)
            .await
            .map_err(ConsumerError::Controller)?;
        Ok(())
    }

    pub async fn unsubscribe(&mut self) -> Result<(), ConsumerError<B>> {
        match self.state {
            ConsumerState::Active => {
                self.state = ConsumerState::Unsubscribing;
                self.controller.unsubscribe(self.subscription_id).await?;
                Ok(())
            }
            ConsumerState::Unsubscribing | ConsumerState::Unsubscribed => {
                // Already unsubscribed, this is fine
                Ok(())
            }
        }
    }

    #[cfg(test)]
    pub fn subscription_id(&self) -> SubscriptionId {
        self.subscription_id
    }
}

impl<B> Stream for Consumer<B>
where
    B: Backend,
{
    type Item = Result<StreamEvent, ConsumerError<B>>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if matches!(self.state, ConsumerState::Unsubscribed) {
            return Poll::Ready(None);
        }

        match Pin::new(&mut self.event_stream).poll_next(cx) {
            Poll::Ready(Some(Ok(event))) => {
                if matches!(event, StreamEvent::Unsubscribed)
                    && matches!(self.state, ConsumerState::Unsubscribing)
                {
                    self.state = ConsumerState::Unsubscribed;
                    // Return the Unsubscribed event to the user, then end stream on next poll.
                    Poll::Ready(Some(Ok(event)))
                } else {
                    // Forward all other events normally.
                    Poll::Ready(Some(Ok(event)))
                }
            }
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
