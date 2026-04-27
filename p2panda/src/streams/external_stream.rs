// SPDX-License-Identifier: MIT OR Apache-2.0

use std::pin::Pin;
use std::task::{Context, Poll};

use futures_util::stream::SelectAll;
use futures_util::stream::{BoxStream, once};
use futures_util::{FutureExt, Stream, StreamExt};
use tokio::sync::oneshot;

use crate::operation::Operation;

pub type SessionId = u64;

/// Set of currently active external streams.
#[derive(Default)]
pub(crate) struct ExternalStream {
    streams: SelectAll<BoxStream<'static, ExternalStreamEvent>>,
    next_session_id: SessionId,
}

impl ExternalStream {
    /// Insert a new stream into the set, it is assigned the next available session id, and a
    /// future is returned which will complete when the stream has ended and all operations are
    /// processed.
    pub fn insert(&mut self, stream: BoxStream<'static, Operation>) -> ExternalStreamFuture {
        let session_id = self.next_session_id();
        let (complete_tx, complete_rx) = oneshot::channel::<()>();
        let future = ExternalStreamFuture {
            session_id,
            complete_rx,
        };

        let event_stream = once(async move { ExternalStreamEvent::Start { session_id } })
            .chain(stream.map(move |operation| ExternalStreamEvent::Operation {
                session_id,
                operation: Box::new(operation),
            }))
            .chain(once(async move {
                let _ = complete_tx.send(());
                ExternalStreamEvent::End { session_id }
            }));

        self.streams.push(Box::pin(event_stream));

        future
    }

    fn next_session_id(&mut self) -> SessionId {
        let next = self.next_session_id;
        self.next_session_id += 1;
        next
    }
}

impl Stream for ExternalStream {
    type Item = ExternalStreamEvent;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.streams.poll_next_unpin(cx)
    }
}

pub(crate) enum ExternalStreamEvent {
    Start {
        session_id: SessionId,
    },
    Operation {
        session_id: SessionId,
        operation: Box<Operation>,
    },
    End {
        session_id: SessionId,
    },
}

/// Future which can be awaited to find out when external stream has completed processing.
#[derive(Debug)]
pub struct ExternalStreamFuture {
    session_id: SessionId,
    complete_rx: oneshot::Receiver<()>,
}

impl ExternalStreamFuture {
    pub fn session_id(&self) -> SessionId {
        self.session_id
    }
}

impl Future for ExternalStreamFuture {
    type Output = Result<(), oneshot::error::RecvError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.complete_rx.poll_unpin(cx)
    }
}
