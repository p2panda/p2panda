// SPDX-License-Identifier: MIT OR Apache-2.0

use std::pin::Pin;
use std::sync::{Arc, RwLock};
use std::task::{Context, Poll};

use futures_util::FutureExt;
use futures_util::future::{self, BoxFuture};
use p2panda_core::Topic;
use p2panda_net::utils::ShortFormat;
use thiserror::Error;
use tracing::warn;

use crate::egress::delivery::ToDelivery;
use crate::egress::{
    DispatchConfig, EgressInner, EventDeliveryPolicy, EventProcessingPolicy, StreamType,
};
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
                EventDeliveryPolicy::OnlySpaces => {
                    inner.by_stream_type(StreamType::Space).cloned().collect()
                }
            }
            .into_iter()
            .filter_map(|entry| Some((entry.topic, entry.to_delivery?)))
            .collect::<Vec<(Topic, ToDelivery)>>()
        };

        // Delivery is best-effort, failures are logged but don't fail the dispatch.
        let mut delivery_rxs = Vec::with_capacity(to_delivery.len());
        for (topic, tx) in to_delivery {
            match tx.publish(operation.clone()).await {
                Ok(result_rx) => delivery_rxs.push((topic, result_rx)),
                Err(_) => {
                    // Receiver dropped, the registration will clean up the entry.
                    warn!(topic = %topic.fmt_short(), "delivery stream closed before dispatch")
                }
            }
        }

        // Event Processing.
        let to_processing = {
            let inner = self.inner.read().expect("lock poisoned by other thread");

            match config.processing {
                EventProcessingPolicy::Disabled => None,
                EventProcessingPolicy::Topic(topic) => inner
                    .topic(&topic)
                    .and_then(|active| active.to_processing)
                    .map(|tx| (topic, tx)),
            }
        };

        // Processing is strict, if the stream drops during dispatch return an error.
        let processing_rx = match to_processing {
            Some((topic, tx)) => {
                let event = to_event(operation, topic, spaces_events);
                Some((topic, tx.publish(event).await.ok()))
            }
            None => None,
        };

        let delivery_count = delivery_rxs.len();
        let processing_count = if matches!(processing_rx, Some((_, Some(_)))) {
            1
        } else {
            0
        };

        let fut = async move {
            let deliveries = delivery_rxs
                .into_iter()
                .map(|(topic, rx)| rx.map(move |result| (topic, result)));

            for (topic, result) in future::join_all(deliveries).await {
                if result.is_err() {
                    // Receiver dropped, the registration will clean up the entry.
                    warn!(topic = %topic.fmt_short(), "delivery stream closed during dispatch");
                }
            }

            match processing_rx {
                None => Ok(None),
                Some((topic, None)) => Err(DispatchError::Processing(topic)),
                Some((topic, Some(rx))) => rx
                    .await
                    .map(Some)
                    .map_err(|_| DispatchError::Processing(topic)),
            }
        };

        DispatchFuture {
            delivery_count,
            processing_count,
            fut: fut.boxed(),
        }
    }
}

fn to_event(operation: Operation, topic: Topic, spaces_events: Option<Vec<SpacesEvent>>) -> Event {
    let source = Source::Egress;

    let log_id = operation.header.extensions.log_id();
    let prune_flag = operation.header.extensions.prune_flag();
    let spaces_args = operation
        .header
        .extensions
        .spaces_args(operation.body.as_ref());

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

/// Resolves when all delivery and processing layers handled the dispatched operation.
///
/// Returns the processed event if the operation was sent to a processing layer.
pub struct DispatchFuture {
    /// Number of delivery layers the operation was sent to.
    pub delivery_count: usize,

    /// Number of processing layers the operation was sent to.
    pub processing_count: usize,

    /// Future executing the dispatch to delivery- or processing layers.
    fut: BoxFuture<'static, Result<Option<Event>, DispatchError>>,
}

impl Future for DispatchFuture {
    type Output = Result<Option<Event>, DispatchError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.fut.poll_unpin(cx)
    }
}

#[derive(Debug, Error)]
pub enum DispatchError {
    #[error("processing stream for topic {0} closed before operation was processed")]
    Processing(Topic),
}
