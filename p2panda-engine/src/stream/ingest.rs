// SPDX-License-Identifier: AGPL-3.0-or-later

use std::future::Future;
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
use crate::operation::{ingest_operation, IngestError, IngestResult};
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
    type Item = Result<Operation<E>, IngestError>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut store = self.store.clone();
        let mut this = self.project();

        let res = match this.stream.as_mut().poll_next(cx) {
            Poll::Ready(operation) => operation,
            Poll::Pending => ready!(this.ooo_buffer_rx.as_mut().poll_next(cx)),
        };

        let Some((header, body)) = res else {
            // Either external stream or buffer stream or ended, so we stop here as well
            return Poll::Ready(None);
        };

        let ingest_fut = async { ingest_operation::<S, E>(&mut store, header, body).await };
        pin_utils::pin_mut!(ingest_fut);

        let res = ready!(ingest_fut.poll(cx));

        match res {
            Ok(IngestResult::Retry(header, body, _)) => {
                if let Err(_) = this.ooo_buffer_tx.start_send((header, body)) {
                    Poll::Ready(None)
                } else {
                    Poll::Pending
                }
            }
            Ok(IngestResult::Complete(operation)) => Poll::Ready(Some(Ok(operation))),
            Err(err) => Poll::Ready(Some(Err(err))),
        }
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
mod tests {
    use futures_util::StreamExt;
    use p2panda_store::MemoryStore;
    use pin_utils::pin_mut;

    use crate::stream::decode::DecodeExt;
    use crate::test_utils::{mock_stream, Extensions};
    use crate::StreamName;

    use super::IngestExt;

    #[tokio::test]
    async fn decode() {
        let store = MemoryStore::<StreamName, Extensions>::new();

        let stream = mock_stream()
            .take(5)
            .decode()
            .filter_map(|item| async {
                match item {
                    Ok((header, body)) => Some((header, body)),
                    Err(_) => None,
                }
            })
            .ingest(store, 16);

        pin_mut!(stream);

        while let Some(test) = stream.next().await {
            println!("{test:?}");
        }
    }
}
