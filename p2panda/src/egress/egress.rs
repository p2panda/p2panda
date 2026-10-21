// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};

use p2panda_core::Topic;
use thiserror::Error;

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

    pub fn spaces(&self) -> Vec<EgressTopic> {
        self.active
            .values()
            .filter_map(|active| {
                if active.stream_type == StreamType::Space {
                    Some(active.clone())
                } else {
                    None
                }
            })
            .collect()
    }

    fn validate_processing(
        &mut self,
        topic: Topic,
        stream_type: StreamType,
    ) -> Result<(), EgressError> {
        if let Some(active) = self.active.get_mut(&topic) {
            if active.to_processing.is_some() {
                return Err(EgressError::DuplicateProcessing(topic));
            }

            if active.stream_type != stream_type {
                return Err(EgressError::StreamTypeMismatch(
                    active.stream_type,
                    stream_type,
                ));
            }
        }

        Ok(())
    }

    fn validate_delivery(
        &mut self,
        topic: Topic,
        stream_type: StreamType,
    ) -> Result<(), EgressError> {
        if let Some(active) = self.active.get_mut(&topic) {
            if active.to_delivery.is_some() {
                return Err(EgressError::DuplicateDelivery(topic));
            }

            if active.stream_type != stream_type {
                return Err(EgressError::StreamTypeMismatch(
                    active.stream_type,
                    stream_type,
                ));
            }
        }

        Ok(())
    }

    fn register_processing(
        &mut self,
        topic: Topic,
        stream_type: StreamType,
        tx: ToProcessing,
    ) -> Result<(), EgressError> {
        debug_assert!(self.validate_processing(topic, stream_type).is_ok());

        if let Some(active) = self.active.get_mut(&topic) {
            active.to_processing = Some(tx);
        } else {
            self.active.insert(
                topic,
                EgressTopic {
                    topic,
                    stream_type,
                    to_processing: Some(tx),
                    to_delivery: None,
                },
            );
        }

        Ok(())
    }

    fn register_delivery(
        &mut self,
        topic: Topic,
        stream_type: StreamType,
        tx: ToDelivery,
    ) -> Result<(), EgressError> {
        debug_assert!(self.validate_delivery(topic, stream_type).is_ok());

        if let Some(active) = self.active.get_mut(&topic) {
            active.to_delivery = Some(tx);
        } else {
            self.active.insert(
                topic,
                EgressTopic {
                    topic,
                    stream_type,
                    to_processing: None,
                    to_delivery: Some(tx),
                },
            );
        }

        Ok(())
    }

    pub fn remove_processing(&mut self, topic: &Topic) {
        let mut is_active = true;

        if let Some(entry) = self.active.get_mut(topic) {
            // Remove event processing tx.
            entry.to_processing = None;
            is_active = entry.is_active();
        }

        // Clean up after the last tx got removed as well.
        if !is_active {
            self.active.remove(topic);
        }
    }

    pub fn remove_delivery(&mut self, topic: &Topic) {
        let mut is_active = true;

        if let Some(entry) = self.active.get_mut(topic) {
            // Remove event delivery tx.
            entry.to_delivery = None;
            is_active = entry.is_active();
        }

        // Clean up after the last tx got removed as well.
        if !is_active {
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

    pub fn register_processing(
        &self,
        topic: Topic,
        stream_type: StreamType,
    ) -> Result<ProcessFromEgress, EgressError> {
        let mut inner = self.inner.write().expect("lock poisoned by other thread");
        inner.validate_processing(topic, stream_type)?;

        let (tx, rx) = ToProcessing::new(self.inner.clone(), topic);
        inner.register_processing(topic, stream_type, tx)?;

        Ok(rx)
    }

    pub fn register_delivery(
        &self,
        topic: Topic,
        stream_type: StreamType,
    ) -> Result<DeliverFromEgress, EgressError> {
        let mut inner = self.inner.write().expect("lock poisoned by other thread");
        inner.validate_delivery(topic, stream_type)?;

        let (tx, rx) = ToDelivery::new(self.inner.clone(), topic);
        inner.register_delivery(topic, stream_type, tx)?;

        Ok(rx)
    }

    pub fn group_topics(&self) -> HashSet<Topic> {
        let inner = self.inner.read().expect("lock poisoned by other thread");
        inner
            .active
            .values()
            .filter_map(|active| {
                if active.stream_type == StreamType::Group {
                    Some(active.topic)
                } else {
                    None
                }
            })
            .collect()
    }

    pub fn space_topics(&self) -> HashSet<Topic> {
        let inner = self.inner.read().expect("lock poisoned by other thread");
        inner
            .active
            .values()
            .filter_map(|active| {
                if active.stream_type == StreamType::Space {
                    Some(active.topic)
                } else {
                    None
                }
            })
            .collect()
    }
}

#[derive(Debug, Error)]
pub enum EgressError {
    #[error("processing layer is already registered for topic {0}")]
    DuplicateProcessing(Topic),

    #[error("delivery layer is already registered for topic {0}")]
    DuplicateDelivery(Topic),

    #[error("topic was previously registered with different stream-type, expected: {0}, got: {0}")]
    StreamTypeMismatch(StreamType, StreamType),
}

#[cfg(test)]
mod tests {
    use std::assert_matches;

    use p2panda_core::Topic;

    use crate::egress::{Egress, EgressError, StreamType};

    #[tokio::test]
    async fn duplicate_or_mismatching_streams() {
        let egress = Egress::new();
        let topic = Topic::random();

        let rx_processing = egress
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

        let rx_delivery = egress.register_delivery(topic, StreamType::Topic).unwrap();

        assert_matches!(
            egress.register_delivery(topic, StreamType::Topic),
            Err(EgressError::DuplicateDelivery(_)),
            "should fail when trying to register same delivery again",
        );

        // Dropping both rx should also free the inner topic handle.
        drop(rx_processing);
        drop(rx_delivery);

        assert!(egress.register_delivery(topic, StreamType::Topic).is_ok());
        assert!(egress.register_processing(topic, StreamType::Topic).is_ok());
    }
}
