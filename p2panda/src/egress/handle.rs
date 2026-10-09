// SPDX-License-Identifier: MIT OR Apache-2.0

use std::pin::Pin;
use std::sync::{Arc, RwLock};
use std::task::{Context, Poll};

use futures_util::{FutureExt, future, ready};
use p2panda_core::Topic;
use thiserror::Error;
use tokio::sync::{mpsc, oneshot};

use crate::egress::delivery::ToDelivery;
use crate::egress::{DispatchConfig, EgressInner, EventDeliveryPolicy, EventProcessingPolicy};
use crate::operation::Operation;
use crate::spaces::types::SpacesEvent;
use crate::streams::{Event, Source};

#[derive(Clone, Debug)]
pub struct EgressHandle {
    pub(crate) inner: Arc<RwLock<EgressInner>>,
}

impl EgressHandle {
    pub async fn dispatch(&self, operation: Operation, topic: Topic) -> DispatchFuture {
        let config = DispatchConfig::topic(topic);
        self.dispatch_inner(operation, &config, None).await
    }

    pub async fn dispatch_with_config(
        &self,
        operation: Operation,
        config: &DispatchConfig,
    ) -> DispatchFuture {
        self.dispatch_inner(operation, config, None).await
    }

    pub async fn dispatch_with_spaces_events(
        &self,
        operation: Operation,
        topic: Topic,
        spaces_events: Option<Vec<SpacesEvent>>,
    ) -> DispatchFuture {
        let config = DispatchConfig::topic(topic);
        self.dispatch_inner(operation, &config, spaces_events).await
    }

    async fn dispatch_inner(
        &self,
        operation: Operation,
        config: &DispatchConfig,
        spaces_events: Option<Vec<SpacesEvent>>,
    ) -> DispatchFuture {
        // Event Delivery.
        let to_delivery = {
            let inner = self.inner.read().expect("lock poisoned by other thread");

            match config.delivery {
                EventDeliveryPolicy::Disabled => Vec::new(),
                EventDeliveryPolicy::Topic(topic) => inner
                    .topic(&topic)
                    .map(|tx| Vec::from([tx]))
                    .unwrap_or_default(),
                EventDeliveryPolicy::OnlySpaces => inner.spaces(),
            }
            .into_iter()
            .filter_map(|entry| match entry.to_delivery {
                Some(to_delivery) => Some((entry.topic, to_delivery)),
                None => None,
            })
            .collect::<Vec<(Topic, ToDelivery)>>()
        };

        let mut delivery_futures = Vec::new();
        let delivery_count = to_delivery.len();

        for (topic, tx) in to_delivery {
            match tx.publish(operation.clone()).await {
                Ok(result_rx) => {
                    delivery_futures.push(result_rx);
                }
                Err(mpsc::error::SendError(_)) => {
                    let mut inner = self.inner.write().expect("lock poisoned by other thread");
                    inner.remove_delivery(&topic);
                }
            }
        }

        // Event Processing.
        let to_processing = {
            let inner = self.inner.read().unwrap();

            match config.processing {
                EventProcessingPolicy::Disabled => None,
                EventProcessingPolicy::Topic(topic) => inner
                    .topic(&topic)
                    .and_then(|active| active.to_processing)
                    .map(|tx| (topic, tx)),
            }
        };

        let mut processing_futures = Vec::new();
        let processing_count = if to_processing.is_some() { 1 } else { 0 };

        if let Some((topic, tx)) = to_processing {
            let event = to_event(operation.clone(), topic, spaces_events.clone());

            match tx.publish(event).await {
                Ok(result_rx) => {
                    processing_futures.push(result_rx);
                }
                Err(mpsc::error::SendError(_)) => {
                    let mut inner = self.inner.write().expect("lock poisoned by other thread");
                    inner.remove_processing(&topic);
                }
            }
        }

        DispatchFuture {
            delivery_count,
            delivery_fut: future::try_join_all(delivery_futures),
            processing_count,
            processing_fut: future::try_join_all(processing_futures),
        }
    }
}

fn to_event(operation: Operation, topic: Topic, spaces_events: Option<Vec<SpacesEvent>>) -> Event {
    let source = Source::Egress;

    let log_id = operation.header.extensions.log_id();
    let prune_flag = operation.header.extensions.prune_flag();
    let spaces_args = operation.header.extensions.spaces_args();

    Event::new(
        operation,
        source,
        log_id,
        topic,
        prune_flag,
        spaces_args,
        spaces_events,
    )
}

#[derive(Debug)]
pub struct DispatchFuture {
    pub delivery_count: usize,
    delivery_fut: future::TryJoinAll<oneshot::Receiver<()>>,
    pub processing_count: usize,
    processing_fut: future::TryJoinAll<oneshot::Receiver<Event>>,
}

impl Future for DispatchFuture {
    type Output = Result<(), DispatchError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match ready!(self.delivery_fut.poll_unpin(cx)) {
            Ok(_) => match ready!(self.processing_fut.poll_unpin(cx)) {
                Ok(_) => Poll::Ready(Ok(())),
                Err(err) => Poll::Ready(Err(err.into())),
            },
            Err(err) => Poll::Ready(Err(err.into())),
        }
    }
}

#[derive(Debug, Error)]
pub enum DispatchError {
    #[error("receiving event-delivery or -processing results failed: {0}")]
    RecvResult(#[from] oneshot::error::RecvError),
}
