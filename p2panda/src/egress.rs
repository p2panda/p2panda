// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use futures_util::{FutureExt, future, ready};
use p2panda_core::Topic;
use p2panda_net::utils::ShortFormat;
use thiserror::Error;
use tokio::sync::{RwLock, oneshot};
use tracing::error;

use crate::operation::Operation;
use crate::spaces::types::SpacesEvent;
use crate::streams::{Event, ImportLocalTx, Source};

/// Configure event-delivery & -processing policies for locally forged or remotely received
/// operations.
#[derive(Clone, Default, Debug)]
pub struct EgressConfig {
    /// Determines if and how the operation is pushed to the event delivery layer.
    pub delivery: EventDeliveryPolicy,

    /// Determines if an operation is processed by an event processing pipeline for one or more
    /// topic streams.
    pub processing: EventProcessingPolicy,
}

impl EgressConfig {
    pub fn topic(topic: Topic) -> Self {
        Self {
            delivery: EventDeliveryPolicy::Topic(topic),
            processing: EventProcessingPolicy::Topic(topic),
        }
    }
}

/// Determines if and how the operation is pushed to the event delivery layer.
///
/// If no active event delivery channels are available for the given scope, this can be a no-op.
#[derive(Clone, Default, Debug)]
pub enum EventDeliveryPolicy {
    /// Do not immediately push operation to any event delivery layer.
    ///
    /// This option does not necessarily control if the operation will _never_ be read by the
    /// network. It merely determines if a networking layer will receive the operation _immediately_
    /// (for example to eagerly push it to all active nodes, flood it in the mesh, etc.). Event
    /// delivery solutions usually have access to the database of all operations and will query the
    /// operation latest from there.
    #[default]
    Disabled,

    /// Push operation to event delivery channel concerned with this topic (if it exists).
    Topic(Topic),

    /// Push operation to all channels which are currently exchanging over encrypted spaces.
    OnlySpaces,
}

/// Determines if an operation is processed by an event processing pipeline for one or more topic
/// streams.
///
/// If no active event processing is available for the given scope, this can be a no-op.
#[derive(Clone, Default, Debug)]
pub enum EventProcessingPolicy {
    /// Do not process operation immediately.
    ///
    /// Note that this option will not block processing entirely for this operation. It will be
    /// processed next time a related topic stream is established and will be handled as part of the
    /// replay (since it wasn't acked yet).
    #[default]
    Disabled,

    /// Process operation in pipeline for a particular topic (if active stream exists).
    Topic(Topic),
}

#[allow(clippy::large_enum_variant)]
pub enum EgressDestination {
    Delivery(Operation),
    Processing(Event),
}

impl EgressDestination {
    pub fn operation(&self) -> &Operation {
        match self {
            Self::Delivery(operation) => operation,
            Self::Processing(event) => &event.operation,
        }
    }
}

#[derive(Debug)]
pub struct Egress {
    inner: Arc<RwLock<EgressInner>>,
}

#[derive(Debug)]
struct EgressInner {
    handles: HashMap<Topic, ImportLocalTx>,
    space_ids: HashSet<Topic>,
}

impl EgressInner {
    fn topic(&self, topic: Topic) -> Option<(Topic, ImportLocalTx)> {
        // Clone tx to be able to drop lock and not keep a reference into it.
        self.handles.get(&topic).map(|tx| (topic, tx.clone()))
    }

    fn spaces(&self) -> Vec<(Topic, ImportLocalTx)> {
        let mut result = Vec::new();

        for id in &self.space_ids {
            if let Some(tx) = self.handles.get(id) {
                result.push((*id, tx.clone()));
            }
        }

        result
    }
}

impl Egress {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(EgressInner {
                handles: HashMap::with_capacity(16),
                space_ids: HashSet::with_capacity(8),
            })),
        }
    }

    pub fn handle(&self) -> EgressHandle {
        EgressHandle {
            inner: self.inner.clone(),
        }
    }

    pub async fn add_stream(&self, topic: Topic, is_space: bool, import_tx: ImportLocalTx) -> bool {
        let mut inner = self.inner.write().await;

        if let Entry::Vacant(entry) = inner.handles.entry(topic) {
            entry.insert(import_tx);

            if is_space {
                inner.space_ids.insert(topic);
            }

            true
        } else {
            false
        }
    }

    // TODO: Make sure we're removing streams as well.
    #[allow(unused)]
    pub async fn remove_stream(&self, topic: Topic) -> bool {
        let mut inner = self.inner.write().await;
        inner.space_ids.remove(&topic);
        inner.handles.remove(&topic).is_some()
    }
}

#[derive(Clone, Debug)]
pub struct EgressHandle {
    inner: Arc<RwLock<EgressInner>>,
}

impl EgressHandle {
    pub async fn submit(
        &self,
        operation: Operation,
        topic: Topic,
    ) -> Result<SubmitFuture, SubmitError> {
        let config = EgressConfig::topic(topic);
        self.submit_inner(operation, &config, None).await
    }

    pub async fn submit_with_config(
        &self,
        operation: Operation,
        config: &EgressConfig,
    ) -> Result<SubmitFuture, SubmitError> {
        self.submit_inner(operation, config, None).await
    }

    pub async fn submit_with_spaces_events(
        &self,
        operation: Operation,
        topic: Topic,
        spaces_events: Option<Vec<SpacesEvent>>,
    ) -> Result<SubmitFuture, SubmitError> {
        let config = EgressConfig::topic(topic);
        self.submit_inner(operation, &config, spaces_events).await
    }

    async fn submit_inner(
        &self,
        operation: Operation,
        config: &EgressConfig,
        spaces_events: Option<Vec<SpacesEvent>>,
    ) -> Result<SubmitFuture, SubmitError> {
        let mut broken_channels = Vec::new();

        // Event Delivery.
        let to_delivery = {
            let inner = self.inner.read().await;

            match config.delivery {
                EventDeliveryPolicy::Disabled => Vec::new(),
                EventDeliveryPolicy::Topic(topic) => inner
                    .topic(topic)
                    .map(|tx| Vec::from([tx]))
                    .unwrap_or_default(),
                EventDeliveryPolicy::OnlySpaces => inner.spaces(),
            }
        };

        let mut delivery_futures = Vec::new();
        let delivery_count = to_delivery.len();

        for (topic, tx) in to_delivery {
            match send_to_import_tx(EgressDestination::Delivery(operation.clone()), &topic, &tx)
                .await
            {
                Err(SubmitError::SendEvent(_)) => {
                    broken_channels.push(topic);
                }
                Err(err) => {
                    return Err(err);
                }
                Ok(done_rx) => {
                    delivery_futures.push(done_rx);
                }
            }
        }

        // Event Processing.
        let to_processing = {
            let inner = self.inner.read().await;

            match config.processing {
                EventProcessingPolicy::Disabled => None,
                EventProcessingPolicy::Topic(topic) => {
                    inner.topic(topic).map(|tx| Some(tx)).unwrap_or_default()
                }
            }
        };

        let mut processing_futures = Vec::new();
        let processing_count = if to_processing.is_some() { 1 } else { 0 };

        if let Some((topic, tx)) = to_processing {
            let event = to_event(operation.clone(), topic, spaces_events.clone());
            match send_to_import_tx(EgressDestination::Processing(event), &topic, &tx).await {
                Err(SubmitError::SendEvent(_)) => {
                    broken_channels.push(topic);
                }
                Err(err) => {
                    return Err(err);
                }
                Ok(done_rx) => {
                    processing_futures.push(done_rx);
                }
            }
        }

        if !broken_channels.is_empty() {
            let mut inner = self.inner.write().await;

            for topic in broken_channels.iter() {
                inner.space_ids.remove(topic);
                inner.handles.remove(topic);
            }
        }

        Ok(SubmitFuture {
            delivery_count,
            delivery_fut: future::try_join_all(delivery_futures),
            processing_count,
            processing_fut: future::try_join_all(processing_futures),
        })
    }
}

async fn send_to_import_tx(
    destination: EgressDestination,
    topic: &Topic,
    import_local_tx: &ImportLocalTx,
) -> Result<oneshot::Receiver<()>, SubmitError> {
    let (ready_tx, ready_rx) = oneshot::channel::<()>();

    if let Err(err) = import_local_tx.send((destination, ready_tx)).await {
        error!(
            topic = %topic.fmt_short(),
            "sending message failed due to error: {err}"
        );

        return Err(SubmitError::SendEvent(err.to_string()));
    }

    Ok(ready_rx)
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

#[derive(Debug, Error)]
pub enum SubmitError {
    #[error("sending operation to event-delivery or -processing failed: {0}")]
    SendEvent(String),

    #[error("receiving event-delivery or -processing results failed: {0}")]
    RecvResult(#[from] oneshot::error::RecvError),
}

#[derive(Debug)]
pub struct SubmitFuture {
    pub delivery_count: usize,
    delivery_fut: future::TryJoinAll<oneshot::Receiver<()>>,
    pub processing_count: usize,
    processing_fut: future::TryJoinAll<oneshot::Receiver<()>>,
}

impl Future for SubmitFuture {
    type Output = Result<(), EgressError>;

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
pub enum EgressError {
    #[error("receiving event-delivery or -processing results failed: {0}")]
    RecvResult(#[from] oneshot::error::RecvError),
}
