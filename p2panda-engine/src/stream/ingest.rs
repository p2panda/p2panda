// SPDX-License-Identifier: AGPL-3.0-or-later

use std::future::Future;
use std::marker::PhantomData;
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

use crate::extensions::PruneFlag;
use crate::macros::{delegate_access_inner, delegate_sink};
use crate::operation::{ingest_operation, IngestError, IngestResult};

pub trait IngestExt<S, L, E>: Stream<Item = (Header<E>, Option<Body>)> {
    fn ingest(self, store: S, ooo_buffer_size: usize) -> Ingest<Self, S, L, E>
    where
        S: OperationStore<L, E> + LogStore<L, E>,
        E: Clone + Serialize + DeserializeOwned + Extension<L> + Extension<PruneFlag>,
        Self: Sized,
    {
        Ingest::new(self, store, ooo_buffer_size)
    }
}

impl<T: ?Sized, S, L, E> IngestExt<S, L, E> for T where T: Stream<Item = (Header<E>, Option<Body>)> {}

#[derive(Debug)]
#[pin_project]
#[must_use = "streams do nothing unless polled"]
pub struct Ingest<St, S, L, E>
where
    St: Stream<Item = (Header<E>, Option<Body>)>,
    E: Clone + Serialize + DeserializeOwned + Extension<L> + Extension<PruneFlag>,
    S: OperationStore<L, E> + LogStore<L, E>,
{
    #[pin]
    stream: Fuse<St>,
    store: S,
    ooo_buffer_size: usize,
    ooo_buffer_tx: mpsc::Sender<IngestAttempt<E>>,
    #[pin]
    ooo_buffer_rx: mpsc::Receiver<IngestAttempt<E>>,
    _marker: PhantomData<L>,
}

impl<St, S, L, E> Ingest<St, S, L, E>
where
    St: Stream<Item = (Header<E>, Option<Body>)>,
    S: OperationStore<L, E> + LogStore<L, E>,
    E: Clone + Serialize + DeserializeOwned + Extension<L> + Extension<PruneFlag>,
{
    pub(super) fn new(stream: St, store: S, ooo_buffer_size: usize) -> Ingest<St, S, L, E> {
        let (ooo_buffer_tx, ooo_buffer_rx) = mpsc::channel::<IngestAttempt<E>>(ooo_buffer_size);

        Ingest {
            store,
            stream: stream.fuse(),
            ooo_buffer_size,
            ooo_buffer_tx,
            ooo_buffer_rx,
            _marker: PhantomData,
        }
    }

    delegate_access_inner!(stream, St, (.));
}

impl<St, S, L, E> Stream for Ingest<St, S, L, E>
where
    St: Stream<Item = (Header<E>, Option<Body>)>,
    S: Clone + OperationStore<L, E> + LogStore<L, E>,
    E: Clone + Serialize + DeserializeOwned + Extension<L> + Extension<PruneFlag>,
{
    type Item = Result<Operation<E>, IngestError<E>>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut store = self.store.clone();
        let mut this = self.project();

        loop {
            // 1. Pull in the next item from the external stream, if there's none or the stream got
            //    terminated, check the internal out-of-order buffer next. We always prefer pulling
            //    from the external stream first as freshly incoming data should be prioritized.
            //
            //    If the buffer ran full we prioritize pulling from it first to re-attempt ingest
            //    for this item.
            let res = {
                if this.ooo_buffer_rx.size_hint().0 == *this.ooo_buffer_size {
                    ready!(this.ooo_buffer_rx.as_mut().poll_next(cx))
                } else {
                    match this.stream.as_mut().poll_next(cx) {
                        Poll::Ready(Some((header, body))) => Some(IngestAttempt(header, body, 1)),
                        Poll::Pending => ready!(this.ooo_buffer_rx.as_mut().poll_next(cx)),
                        Poll::Ready(None) => match this.ooo_buffer_rx.as_mut().poll_next(cx) {
                            Poll::Ready(Some(attempt)) => Some(attempt),
                            // If there's no value coming from the buffer _and_ the external stream is
                            // terminated, we can be sure nothing will come anymore
                            Poll::Pending => None,
                            Poll::Ready(None) => None,
                        },
                    }
                }
            };
            let Some(IngestAttempt(header, body, counter)) = res else {
                // Both external stream and buffer stream has ended, so we stop here as well
                return Poll::Ready(None);
            };

            // 2. Validate and check the log-integrity of the incoming operation. If it is valid it
            //    get's persisted and the log optionally pruned.
            let ingest_fut = async { ingest_operation::<S, L, E>(&mut store, header, body).await };
            pin_mut!(ingest_fut);
            let ingest_res = ready!(ingest_fut.poll(cx));

            // 3. If the operation arrived out-of-order we can push it back into the internal
            //    buffer and try again later (attempted for a configured number of times),
            //    otherwise forward the result of ingest to the consumer.
            match ingest_res {
                Ok(IngestResult::Retry(header, body, num_missing)) => {
                    // The number of max. reattempts is equal the size of the buffer. As long as
                    // the buffer is just a FIFO queue it doesn't make sense to optimize over
                    // different parameters as in a worst-case distribution of items (exact
                    // reverse) this will be the max. and min. required bound
                    if counter > *this.ooo_buffer_size {
                        return Poll::Ready(Some(Err(IngestError::MaxAttemptsReached(
                            header,
                            body,
                            num_missing,
                        ))));
                    }

                    // Push operation back into the internal queue, if something goes wrong here
                    // this must be an critical failure
                    let Ok(_) = ready!(this.ooo_buffer_tx.poll_ready(cx)) else {
                        break Poll::Ready(None);
                    };

                    let Ok(_) =
                        this.ooo_buffer_tx
                            .start_send(IngestAttempt(header, body, counter + 1))
                    else {
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

impl<St: FusedStream, S, L, E> FusedStream for Ingest<St, S, L, E>
where
    St: Stream<Item = (Header<E>, Option<Body>)>,
    S: Clone + OperationStore<L, E> + LogStore<L, E>,
    E: Clone + Serialize + DeserializeOwned + Extension<L> + Extension<PruneFlag>,
{
    fn is_terminated(&self) -> bool {
        self.stream.is_terminated() && self.ooo_buffer_rx.is_terminated()
    }
}

impl<St, S, L, E> Sink<(Header<E>, Option<Body>)> for Ingest<St, S, L, E>
where
    St: Stream<Item = (Header<E>, Option<Body>)> + Sink<(Header<E>, Option<Body>)>,
    S: OperationStore<L, E> + LogStore<L, E>,
    E: Clone + Serialize + DeserializeOwned + Extension<L> + Extension<PruneFlag>,
{
    type Error = St::Error;

    delegate_sink!(stream, (Header<E>, Option<Body>));
}

#[derive(Debug)]
struct IngestAttempt<E>(Header<E>, Option<Body>, usize);

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

        let res: Result<Vec<Operation<Extensions>>, IngestError<Extensions>> =
            stream.try_collect().await;
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
            // If the random distribution over the sample set is equal, it makes sense to keep the
            // buffer size at least as big as the sample size (giving each item a chance even if
            // they're in a completly wrong place). Like this we can guarantee that ingest (and
            // this test) will be successful
            .ingest(store, items_num);

        let res: Vec<Operation<Extensions>> = stream.try_collect().await.expect("not fail");
        assert_eq!(res.len(), items_num);
    }
}
