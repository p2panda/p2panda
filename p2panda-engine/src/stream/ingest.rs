// SPDX-License-Identifier: AGPL-3.0-or-later

use std::pin::Pin;

use futures_channel::mpsc;
use futures_util::stream::{Fuse, FusedStream};
use futures_util::task::{Context, Poll};
use futures_util::{ready, Sink, Stream, StreamExt};
use p2panda_core::{Body, Extension, Header, Operation};
use p2panda_store::{LogStore, OperationStore};
use pin_project::pin_project;
use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::macros::{delegate_access_inner, delegate_sink};
use crate::operation::IngestError;
use crate::{PruneFlag, StreamName};

pub trait IngestExt<S, E>: Stream<Item = (Header<E>, Option<Body>)> {
    fn ingest(self, store: S, ooo_buffer_size: usize) -> Ingest<Self, S, E>
    where
        S: OperationStore<StreamName, E> + LogStore<StreamName, E>,
        E: Clone + Serialize + DeserializeOwned + Extension<StreamName> + Extension<PruneFlag>,
        Self: Sized,
    {
        Ingest::new(self, store, ooo_buffer_size)
    }
}

impl<T: ?Sized, S, E> IngestExt<S, E> for T where T: Stream<Item = (Header<E>, Option<Body>)> {}

#[derive(Debug)]
#[pin_project]
#[must_use = "streams do nothing unless polled"]
pub struct Ingest<St, S, E>
where
    St: Stream<Item = (Header<E>, Option<Body>)>,
    E: Clone + Serialize + DeserializeOwned + Extension<StreamName> + Extension<PruneFlag>,
    S: OperationStore<StreamName, E> + LogStore<StreamName, E>,
{
    #[pin]
    stream: Fuse<St>,
    store: S,
    ooo_buffer_tx: mpsc::Sender<(Header<E>, Option<Body>)>,
    #[pin]
    ooo_buffer_rx: mpsc::Receiver<(Header<E>, Option<Body>)>,
}

impl<St, S, E> Ingest<St, S, E>
where
    St: Stream<Item = (Header<E>, Option<Body>)>,
    S: OperationStore<StreamName, E> + LogStore<StreamName, E>,
    E: Clone + Serialize + DeserializeOwned + Extension<StreamName> + Extension<PruneFlag>,
{
    pub(super) fn new(stream: St, store: S, ooo_buffer_size: usize) -> Ingest<St, S, E> {
        let (ooo_buffer_tx, ooo_buffer_rx) =
            mpsc::channel::<(Header<E>, Option<Body>)>(ooo_buffer_size);

        Ingest {
            store,
            stream: stream.fuse(),
            ooo_buffer_tx,
            ooo_buffer_rx,
        }
    }

    delegate_access_inner!(stream, St, (.));
}

impl<St, S, E> Stream for Ingest<St, S, E>
where
    St: Stream<Item = (Header<E>, Option<Body>)>,
    S: Clone + OperationStore<StreamName, E> + LogStore<StreamName, E>,
    E: Clone + Serialize + DeserializeOwned + Extension<StreamName> + Extension<PruneFlag>,
{
    type Item = Result<Operation, IngestError>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut this = self.project();

        let res = match this.stream.as_mut().poll_next(cx) {
            Poll::Ready(operation) => operation,
            Poll::Pending => ready!(this.ooo_buffer_rx.as_mut().poll_next(cx)),
        };

        let Some(operation) = res else {
            // Either external stream or buffer stream or ended, so we stop here as well
            return Poll::Ready(None);
        };

        Poll::Pending
    }
}

impl<St: FusedStream, S, E> FusedStream for Ingest<St, S, E>
where
    St: Stream<Item = (Header<E>, Option<Body>)>,
    S: Clone + OperationStore<StreamName, E> + LogStore<StreamName, E>,
    E: Clone + Serialize + DeserializeOwned + Extension<StreamName> + Extension<PruneFlag>,
{
    fn is_terminated(&self) -> bool {
        self.stream.is_terminated() && self.ooo_buffer_rx.is_terminated()
    }
}

impl<St, S, E> Sink<(Header<E>, Option<Body>)> for Ingest<St, S, E>
where
    St: Stream<Item = (Header<E>, Option<Body>)> + Sink<(Header<E>, Option<Body>)>,
    S: OperationStore<StreamName, E> + LogStore<StreamName, E>,
    E: Clone + Serialize + DeserializeOwned + Extension<StreamName> + Extension<PruneFlag>,
{
    type Error = St::Error;

    delegate_sink!(stream, (Header<E>, Option<Body>));
}

#[cfg(test)]
mod tests {}
