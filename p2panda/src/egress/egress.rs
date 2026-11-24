// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::{HashMap, HashSet};
use std::pin::Pin;
use std::sync::{Arc, RwLock};
use std::task::{Context, Poll};

use futures_util::{Stream, StreamExt};
use p2panda_core::Topic;
use thiserror::Error;
use tokio_stream::wrappers::ReceiverStream;

use crate::egress::delivery::ToDelivery;
use crate::egress::processing::ToProcessing;
use crate::egress::{DeliverFromEgress, EgressHandle, ProcessFromEgress, StreamType};

#[derive(Clone, Debug)]
pub struct Egress {
    inner: Arc<RwLock<EgressInner>>,
}

#[derive(Debug)]
pub struct EgressInner {
    active: HashMap<Topic, EgressTopic>,
}

#[derive(Clone, Debug)]
pub struct EgressTopic {
    pub topic: Topic,
    pub stream_type: StreamType,
    pub to_processing: Option<ToProcessing>,
    pub to_delivery: Option<ToDelivery>,
}

impl EgressTopic {
    fn is_active(&self) -> bool {
        self.to_processing.is_some() || self.to_delivery.is_some()
    }
}

impl EgressInner {
    pub fn topic(&self, topic: &Topic) -> Option<EgressTopic> {
        // Clone rxs to be able to drop lock and not keep a reference into it.
        self.active.get(topic).cloned()
    }

    pub fn by_stream_type(&self, stream_type: StreamType) -> impl Iterator<Item = &EgressTopic> {
        self.active
            .values()
            .filter(move |active| active.stream_type == stream_type)
    }

    /// Returns the entry for this topic, inserts an empty one if it doesn't exist yet.
    fn entry(
        &mut self,
        topic: Topic,
        stream_type: StreamType,
    ) -> Result<&mut EgressTopic, EgressError> {
        let entry = self.active.entry(topic).or_insert_with(|| EgressTopic {
            topic,
            stream_type,
            to_processing: None,
            to_delivery: None,
        });

        if entry.stream_type != stream_type {
            return Err(EgressError::StreamTypeMismatch(
                entry.stream_type,
                stream_type,
            ));
        }

        Ok(entry)
    }

    fn remove(&mut self, topic: &Topic, kind: &RegistrationKind) {
        let Some(entry) = self.active.get_mut(topic) else {
            return;
        };

        match kind {
            RegistrationKind::Processing => entry.to_processing = None,
            RegistrationKind::Delivery => entry.to_delivery = None,
        }

        // Clean up after the last tx got removed as well.
        if !entry.is_active() {
            self.active.remove(topic);
        }
    }
}

impl Egress {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(EgressInner {
                active: HashMap::with_capacity(8),
            })),
        }
    }

    pub fn handle(&self) -> EgressHandle {
        EgressHandle {
            inner: self.inner.clone(),
        }
    }

    /// Registers an event processing layer for this topic.
    ///
    /// The topic stays registered until the returned receiver or its registration is dropped.
    pub fn register_processing(
        &self,
        topic: Topic,
        stream_type: StreamType,
    ) -> Result<ProcessFromEgress, EgressError> {
        let mut inner = self.inner.write().expect("lock poisoned by other thread");
        let entry = inner.entry(topic, stream_type)?;
        if entry.to_processing.is_some() {
            return Err(EgressError::DuplicateProcessing(topic));
        }

        let (tx, rx) = ToProcessing::new();
        entry.to_processing = Some(tx);

        Ok(FromEgress {
            rx,
            registration: self.registration(topic, RegistrationKind::Processing),
        })
    }

    /// Registers an event delivery layer for this topic.
    ///
    /// The topic stays registered until the returned receiver or its registration is dropped.
    pub fn register_delivery(
        &self,
        topic: Topic,
        stream_type: StreamType,
    ) -> Result<DeliverFromEgress, EgressError> {
        let mut inner = self.inner.write().expect("lock poisoned by other thread");
        let entry = inner.entry(topic, stream_type)?;
        if entry.to_delivery.is_some() {
            return Err(EgressError::DuplicateDelivery(topic));
        }

        let (tx, rx) = ToDelivery::new();
        entry.to_delivery = Some(tx);

        Ok(FromEgress {
            rx,
            registration: self.registration(topic, RegistrationKind::Delivery),
        })
    }

    fn registration(&self, topic: Topic, kind: RegistrationKind) -> EgressRegistration {
        EgressRegistration {
            topic,
            kind,
            inner: self.inner.clone(),
        }
    }

    /// Returns all currently registered topics of this stream type.
    pub fn topics(&self, stream_type: StreamType) -> HashSet<Topic> {
        let inner = self.inner.read().expect("lock poisoned by other thread");
        inner
            .by_stream_type(stream_type)
            .map(|active| active.topic)
            .collect()
    }
}

/// Receiving end of a registered processing- or delivery layer.
#[derive(Debug)]
pub struct FromEgress<T> {
    rx: ReceiverStream<T>,
    registration: EgressRegistration,
}

impl<T> FromEgress<T> {
    pub fn into_parts(self) -> (EgressRegistration, ReceiverStream<T>) {
        (self.registration, self.rx)
    }
}

impl<T> Stream for FromEgress<T> {
    type Item = T;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.rx.poll_next_unpin(cx)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.rx.size_hint()
    }
}

/// Removes the registered processing or delivery layer from the egress when dropped.
#[derive(Debug)]
pub struct EgressRegistration {
    topic: Topic,
    kind: RegistrationKind,
    inner: Arc<RwLock<EgressInner>>,
}

#[derive(Debug)]
enum RegistrationKind {
    Processing,
    Delivery,
}

impl Drop for EgressRegistration {
    fn drop(&mut self) {
        if let Ok(mut inner) = self.inner.write() {
            inner.remove(&self.topic, &self.kind);
        }
    }
}

#[derive(Debug, Error)]
pub enum EgressError {
    #[error("processing layer is already registered for topic {0}")]
    DuplicateProcessing(Topic),

    #[error("delivery layer is already registered for topic {0}")]
    DuplicateDelivery(Topic),

    #[error("topic was previously registered with different stream-type, expected: {0}, got: {1}")]
    StreamTypeMismatch(StreamType, StreamType),
}

#[cfg(test)]
mod tests {
    use std::assert_matches;

    use p2panda_core::Topic;

    use crate::egress::StreamType;

    use super::{Egress, EgressError};

    #[tokio::test]
    async fn duplicate_or_mismatching_streams() {
        let egress = Egress::new();
        let topic = Topic::random();

        let processing = egress
            .register_processing(topic, StreamType::Topic)
            .unwrap();

        assert_matches!(
            egress.register_processing(topic, StreamType::Topic),
            Err(EgressError::DuplicateProcessing(_)),
            "should fail when trying to register same processing again",
        );

        assert_matches!(
            egress.register_delivery(topic, StreamType::Space),
            Err(EgressError::StreamTypeMismatch(_, _)),
            "should fail when trying to register with different stream type",
        );

        let delivery = egress.register_delivery(topic, StreamType::Topic).unwrap();

        assert_matches!(
            egress.register_delivery(topic, StreamType::Topic),
            Err(EgressError::DuplicateDelivery(_)),
            "should fail when trying to register same delivery again",
        );

        // Dropping both receivers should also free the inner topic handle.
        drop(processing);
        drop(delivery);

        assert!(egress.register_delivery(topic, StreamType::Topic).is_ok());
        assert!(egress.register_processing(topic, StreamType::Topic).is_ok());
    }

    #[tokio::test]
    async fn unregister_while_rx_still_alive() {
        let egress = Egress::new();
        let topic = Topic::random();

        let (registration, _rx) = egress
            .register_processing(topic, StreamType::Topic)
            .unwrap()
            .into_parts();

        // Receiver might still be held by a task which hasn't been cancelled yet.
        drop(registration);

        assert!(egress.register_processing(topic, StreamType::Topic).is_ok());
    }
}
