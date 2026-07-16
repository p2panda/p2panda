// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fmt::Debug;
use std::pin::Pin;
use std::task::{Context, Poll};

use futures_util::{Stream, StreamExt};
use p2panda_core::{Hash, Topic};
use p2panda_store::SqliteStore;
use p2panda_store::operations::OperationStore;
use serde::{Deserialize, Serialize};
use tokio_stream::wrappers::ReceiverStream;

use crate::streams::StreamEvent;
use crate::streams::acked::{Acked, AckedError};

/// Subscription to events arriving from a topic stream.
///
/// A topic stream emits:
///
/// - Locally created or remotely synced [`ProcessedOperation`] with application messages inside
/// - Topic-scoped system-events, for example if a sync session has begun and how much will be sent
/// - Critical errors such as [`AckedError`] coming from the processing pipeline
///
/// ## Example
///
/// ```no_run
/// use futures_util::StreamExt;
/// use p2panda_core::Topic;
/// # #[tokio::main]
/// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
/// # let node = p2panda::spawn().await?;
/// let topic = Topic::random();
///
/// let (_tx, mut rx) = node.stream::<String>(topic).await?;
///
/// while let Some(stream_event) = rx.next().await {
///     // .. react to topic stream events
/// }
/// #
/// # Ok(())
/// # }
/// ```
#[derive(Debug)]
pub struct StreamSubscription<M> {
    topic: Topic,
    store: SqliteStore,
    acked: Acked,
    stream: ReceiverStream<StreamEvent<M>>,
}

impl<M> StreamSubscription<M> {
    /// Create a new `StreamSubscription`.
    pub fn new(
        topic: Topic,
        store: SqliteStore,
        acked: Acked,
        stream: ReceiverStream<StreamEvent<M>>,
    ) -> Self {
        Self {
            topic,
            store,
            acked,
            stream,
        }
    }

    /// Associated topic.
    pub fn topic(&self) -> Topic {
        self.topic
    }

    /// Explicitly acknowledge operation.
    ///
    /// Fails silently if operation is not known (it might have been pruned, etc.).
    ///
    /// If the [`AckPolicy`] is set to "explicit", users want to call this method _after_
    /// applicaton-level processing has successfully finished. See high-level description in
    /// [`Node::stream`](crate::node::Node::stream) for more details.
    pub async fn ack(&self, id: Hash) -> Result<(), AckedError> {
        if let Some(operation) = OperationStore::<_, _>::get_operation(&self.store, &id).await? {
            self.acked.ack(&operation.header).await?;
        }

        Ok(())
    }
}

impl<M> Stream for StreamSubscription<M>
where
    M: Serialize + for<'a> Deserialize<'a> + Send + 'static,
{
    type Item = StreamEvent<M>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.stream.poll_next_unpin(cx)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.stream.size_hint()
    }
}
