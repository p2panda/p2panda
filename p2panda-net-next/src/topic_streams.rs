// SPDX-License-Identifier: MIT OR Apache-2.0

//! Topic stream types and associated methods.
use anyhow::Result;
use tokio::sync::mpsc;

use crate::TopicId;
use crate::network::{FromNetwork, ToNetwork};

/// A handle to an ephemeral messaging stream.
///
/// The stream can be used to publish messages or to request a subscription.
pub struct EphemeralTopicStream {
    topic_id: TopicId,
    to_topic_tx: mpsc::Sender<ToNetwork>,
}

impl EphemeralTopicStream {
    /// Returns a handle to an ephemeral messaging stream.
    pub fn new(topic_id: TopicId, to_topic_tx: mpsc::Sender<ToNetwork>) -> Self {
        Self {
            topic_id,
            to_topic_tx,
        }
    }

    /// Publishes a message to the stream.
    async fn publish(&self, bytes: Vec<u8>) -> Result<()> {
        let message = ToNetwork::Message { bytes };

        self.to_topic_tx.send(message).await?;

        Ok(())
    }

    /// Subscribes to the stream.
    ///
    /// The returned `EphemeralTopicStreamSubscription` provides a means of receiving messages from
    /// the stream.
    fn subscribe(&self) -> Result<EphemeralTopicStreamSubscription> {
        todo!()
    }

    /// Returns the topic ID of the stream.
    fn topic_id(&self) -> Result<TopicId> {
        Ok(self.topic_id)
    }
}

/// A handle to an ephemeral messaging stream subscription.
///
/// The stream can be used to receive messages from the stream.
struct EphemeralTopicStreamSubscription {
    topic_id: TopicId,
    from_topic_tx: mpsc::Receiver<FromNetwork>,
}

impl EphemeralTopicStreamSubscription {
    /// Unsubscribes from the stream.
    fn unsubscribe(&self) -> Result<()> {
        todo!()
    }
}
