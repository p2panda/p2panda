// SPDX-License-Identifier: MIT OR Apache-2.0

//! Topic stream types and associated methods.
use ractor::{ActorRef, call, registry};
use thiserror::Error;
use tokio::sync::broadcast::Receiver as BroadcastReceiver;
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::mpsc::Sender;
use tokio::sync::mpsc::error::SendError;

use crate::TopicId;
use crate::actors::subscription::ToSubscription;
use crate::network::{FromNetwork, ToNetwork};

#[derive(Debug, Error)]
pub enum TopicStreamError<T> {
    #[error(transparent)]
    Send(#[from] SendError<T>),

    #[error(transparent)]
    Recv(#[from] RecvError),

    #[error("subscription actor is not available to create topic subscription")]
    SubscriptionNotAvailable,
}

/// A handle to an ephemeral messaging stream.
///
/// The stream can be used to publish messages or to request a subscription.
pub struct EphemeralTopicStream {
    topic_id: TopicId,
    to_topic_tx: Sender<ToNetwork>,
}

impl EphemeralTopicStream {
    /// Returns a handle to an ephemeral messaging stream.
    pub fn new(topic_id: TopicId, to_topic_tx: Sender<ToNetwork>) -> Self {
        Self {
            topic_id,
            to_topic_tx,
        }
    }

    /// Publishes a message to the stream.
    pub async fn publish(&self, bytes: Vec<u8>) -> Result<(), TopicStreamError<Vec<u8>>> {
        self.to_topic_tx.send(bytes).await?;

        Ok(())
    }

    /// Subscribes to the stream.
    ///
    /// The returned `EphemeralTopicStreamSubscription` provides a means of receiving messages from
    /// the stream.
    pub async fn subscribe(
        &self,
    ) -> Result<EphemeralTopicStreamSubscription, TopicStreamError<()>> {
        // Get a reference to the subscription actor.
        if let Some(subscription_actor) = registry::where_is("subscription".to_string()) {
            let actor: ActorRef<ToSubscription> = subscription_actor.into();

            // Ask the subscription actor for an ephemeral stream subscriber.
            let subscription = call!(
                actor,
                ToSubscription::ReturnEphemeralSubscription,
                self.topic_id
            )
            .map_err(|_| TopicStreamError::SubscriptionNotAvailable)?;

            Ok(subscription)
        } else {
            Err(TopicStreamError::SubscriptionNotAvailable)
        }
    }

    /// Returns the topic ID of the stream.
    fn topic_id(&self) -> TopicId {
        self.topic_id
    }
}

/// A handle to an ephemeral messaging stream subscription.
///
/// The stream can be used to receive messages from the stream.
pub struct EphemeralTopicStreamSubscription {
    topic_id: TopicId,
    from_topic_rx: BroadcastReceiver<FromNetwork>,
}

impl EphemeralTopicStreamSubscription {
    /// Returns a handle to an ephemeral messaging stream subscriber.
    pub fn new(topic_id: TopicId, from_topic_rx: BroadcastReceiver<FromNetwork>) -> Self {
        Self {
            topic_id,
            from_topic_rx,
        }
    }

    /// Receives the next message from the stream.
    pub async fn recv(&mut self) -> Result<FromNetwork, TopicStreamError<()>> {
        self.from_topic_rx
            .recv()
            .await
            .map_err(|err| TopicStreamError::Recv(err))
    }

    /// Unsubscribes from the stream.
    fn unsubscribe(&self) -> Result<(), ()> {
        // TODO: How to handle unsubscribe?
        //
        // Should there be an `unsubscribe()` method on `EphemeralTopicStreamSubscription` and
        // `EphemeralTopicStream`?
        todo!()
    }
}
