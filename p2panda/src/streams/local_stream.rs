// SPDX-License-Identifier: MIT OR Apache-2.0

use std::pin::Pin;
use std::task::{Context, Poll};

use futures_util::stream::SelectAll;
use futures_util::stream::{BoxStream, once};
use futures_util::{FutureExt, Stream, StreamExt};
use tokio::sync::oneshot;

use crate::operation::Operation;
use crate::spaces::types::SpacesEvent;

/// Set of currently active local streams.
#[derive(Default)]
pub(crate) struct LocalStream {
    streams: SelectAll<BoxStream<'static, LocalStreamEvent>>,
}

pub enum LocalStreamDestination {
    Delivery(Operation),
    Processing(Operation, Vec<SpacesEvent>),
}

impl LocalStreamDestination {
    pub fn processing(operation: Operation) -> Self {
        Self::Processing(operation, vec![])
    }

    pub fn processing_enriched(operation: Operation, events: Vec<SpacesEvent>) -> Self {
        Self::Processing(operation, events)
    }
}

#[cfg(test)]
impl LocalStreamDestination {
    pub fn operation(&self) -> &Operation {
        match self {
            Self::Delivery(operation) => operation,
            Self::Processing(operation, _) => operation,
        }
    }
}

#[allow(clippy::large_enum_variant)]
pub enum LocalStreamEvent {
    Item(LocalStreamDestination),
    End,
}

impl LocalStream {
    /// Insert a new stream into the set, a future is returned which will complete when the stream
    /// has ended and all operations are processed.
    pub fn insert(
        &mut self,
        stream: BoxStream<'static, LocalStreamDestination>,
    ) -> LocalStreamFuture {
        let (complete_tx, complete_rx) = oneshot::channel::<()>();
        let future = LocalStreamFuture { complete_rx };

        let event_stream = stream.map(LocalStreamEvent::Item).chain(once(async move {
            let _ = complete_tx.send(());
            LocalStreamEvent::End
        }));

        self.streams.push(Box::pin(event_stream));

        future
    }
}

impl Stream for LocalStream {
    type Item = LocalStreamEvent;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.streams.poll_next_unpin(cx)
    }
}

/// Future which can be awaited to find out when local stream has completed processing.
#[derive(Debug)]
pub struct LocalStreamFuture {
    complete_rx: oneshot::Receiver<()>,
}

impl Future for LocalStreamFuture {
    type Output = Result<(), oneshot::error::RecvError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.complete_rx.poll_unpin(cx)
    }
}
