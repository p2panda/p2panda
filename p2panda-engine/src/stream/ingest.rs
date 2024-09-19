// SPDX-License-Identifier: AGPL-3.0-or-later

use std::future::Future;
use std::pin::Pin;

use futures_channel::mpsc::{self};
use futures_util::stream::{Fuse, FusedStream};
use futures_util::task::{Context, Poll};
use futures_util::{ready, Sink, Stream, StreamExt};
use p2panda_core::{Body, Extension, Header, Operation};
use p2panda_store::{LogStore, OperationStore};
use pin_project::pin_project;
use pin_utils::pin_mut;
use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::extensions::{PruneFlag, StreamName};
use crate::macros::{delegate_access_inner, delegate_sink};
use crate::operation::{ingest_operation, IngestError, IngestResult};

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

        loop {
            // 1. Pull in the next item from the external stream, if there's none or the stream got
            //    terminated, check the internal out-of-order buffer next. We always prefer pulling
            //    from the external stream first as freshly incoming data should be prioritized.
            let res = match this.stream.as_mut().poll_next(cx) {
                Poll::Ready(Some((header, body))) => Some((header, body)),
                Poll::Pending => ready!(this.ooo_buffer_rx.as_mut().poll_next(cx)),
                Poll::Ready(None) => match this.ooo_buffer_rx.as_mut().poll_next(cx) {
                    Poll::Ready(Some((header, body))) => Some((header, body)),
                    Poll::Ready(None) => None,
                    // If there's no value coming from the buffer _and_ the external stream is
                    // terminated, we can be sure nothing will come anymore
                    Poll::Pending => None,
                },
            };
            let Some((header, body)) = res else {
                // Both external stream and buffer stream has ended, so we stop here as well
                return Poll::Ready(None);
            };

            // 2. Validate and check the log-integrity of the incoming operation. If it is valid it
            //    get's persisted and the log optionally pruned.
            let ingest_fut = async { ingest_operation::<S, E>(&mut store, header, body).await };
            pin_mut!(ingest_fut);
            let ingest_res = ready!(ingest_fut.poll(cx));

            // 3. If the operation arrived out-of-order we can push it back into the internal
            //    buffer and try again later, otherwise forward the result of ingest to the
            //    consumer.
            match ingest_res {
                // @TODO: Remove the item from buffer if we've tried for too many times
                Ok(IngestResult::Retry(header, body, _)) => {
                    // Push operation back into the internal queue, if something goes wrong here
                    // this must be an critical failure
                    let Ok(_) = ready!(this.ooo_buffer_tx.poll_ready(cx)) else {
                        break Poll::Ready(None);
                    };

                    let Ok(_) = this.ooo_buffer_tx.start_send((header, body)) else {
                        break Poll::Ready(None);
                    };

                    continue;
                }
                Ok(IngestResult::Complete(operation)) => {
                    return Poll::Ready(Some(Ok(operation)));
                }
                Err(err) => {
                    // Ingest failed and we want the stream consumers to be aware of that
                    return Poll::Ready(Some(Err(err)));
                }
            }
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
    use futures_util::stream::iter;
    use futures_util::{StreamExt, TryStreamExt};
    use p2panda_core::Operation;
    use p2panda_store::MemoryStore;
    use rand::seq::SliceRandom;

    use crate::extensions::StreamName;
    use crate::operation::{IngestError, RawOperation};
    use crate::stream::decode::DecodeExt;
    use crate::test_utils::{mock_stream, Extensions};

    use super::IngestExt;

    #[tokio::test]
    async fn ingest() {
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

        let res: Result<Vec<Operation<Extensions>>, IngestError> = stream.try_collect().await;
        assert!(res.is_ok());
    }

    #[tokio::test]
    async fn out_of_order() {
        let items_num = 10;
        let store = MemoryStore::<StreamName, Extensions>::new();

        let mut items: Vec<RawOperation> = mock_stream().take(items_num).collect().await;
        items.shuffle(&mut rand::thread_rng());

        let stream = iter(items)
            .decode()
            .filter_map(|item| async {
                match item {
                    Ok((header, body)) => Some((header, body)),
                    Err(_) => None,
                }
            })
            .ingest(store, 32);

        let res: Vec<Operation<Extensions>> = stream.try_collect().await.expect("not fail");
        assert_eq!(res.len(), items_num);
    }
}
