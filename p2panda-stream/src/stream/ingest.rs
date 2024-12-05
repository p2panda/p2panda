// SPDX-License-Identifier: MIT OR Apache-2.0

use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;

use futures_channel::mpsc::{self};
use futures_util::stream::{Fuse, FusedStream};
use futures_util::task::{Context, Poll};
use futures_util::{ready, Sink, Stream, StreamExt};
use p2panda_core::prune::PruneFlag;
use p2panda_core::{Body, Extension, Extensions, Header, Operation};
use p2panda_store::{LogStore, OperationStore};
use pin_project::pin_project;
use pin_utils::pin_mut;

use crate::macros::{delegate_access_inner, delegate_sink};
use crate::operation::{ingest_operation, IngestError, IngestResult};

/// An extension trait for `Stream`s that provides a convenient [`ingest`](IngestExt::ingest)
/// method.
pub trait IngestExt<S, L, E>: Stream<Item = (Header<E>, Option<Body>, Vec<u8>)> {
    /// Checks incoming operations against their log integrity and persists them automatically in a
    /// given store.
    ///
    /// Every given operation needs to implement a "prune flag" in their header as specified by the
    /// p2panda protocol. Ingest will make sure to accordingly validate based on the given prune
    /// status and automatically remove past items from the log.
    ///
    /// This ingest implementation holds an internal buffer for operations which come in "out of
    /// order". The buffer size determines the maximum number of out-of-order operations in a row
    /// this method can handle. This means that given a buffer size of for example 100, we can
    /// handle a worst-case unordered, fully reversed log with 100 items without problem.
    fn ingest(self, store: S, ooo_buffer_size: usize) -> Ingest<Self, S, L, E>
    where
        S: OperationStore<L, E> + LogStore<L, E>,
        E: Extension<L> + Extension<PruneFlag> + Extensions,
        Self: Sized,
    {
        Ingest::new(self, store, ooo_buffer_size)
    }
}

impl<T: ?Sized, S, L, E> IngestExt<S, L, E> for T where
    T: Stream<Item = (Header<E>, Option<Body>, Vec<u8>)>
{
}

/// Stream for the [`ingest`](IngestExt::ingest) method.
#[derive(Debug)]
#[pin_project]
#[must_use = "streams do nothing unless polled"]
pub struct Ingest<St, S, L, E>
where
    St: Stream<Item = (Header<E>, Option<Body>, Vec<u8>)>,
    E: Extension<L> + Extension<PruneFlag> + Extensions,
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
    St: Stream<Item = (Header<E>, Option<Body>, Vec<u8>)>,
    S: OperationStore<L, E> + LogStore<L, E>,
    E: Extension<L> + Extension<PruneFlag> + Extensions,
{
    pub(super) fn new(stream: St, store: S, ooo_buffer_size: usize) -> Ingest<St, S, L, E> {
        // @TODO(adz): We can optimize for the internal out-of-order buffer even more as it's FIFO
        // nature is not optimal. A sorted list (by seq num, maybe even grouped by public key)
        // might be more efficient, though I'm not sure about optimal implementations yet, so
        // benchmarks and more real-world experience might make sense before we attempt any of
        // this.
        //
        // Also, using an mpsc for the internal buffer seems overkill.
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
    St: Stream<Item = (Header<E>, Option<Body>, Vec<u8>)>,
    S: OperationStore<L, E> + LogStore<L, E>,
    E: Extension<L> + Extension<PruneFlag> + Extensions,
{
    type Item = Result<Operation<E>, IngestError>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut store = self.store.clone();
        let mut this = self.project();

        loop {
            // 1. Pull in the next item from the external stream or out-of-order buffer.
            let res = {
                // If the buffer ran full we prioritize pulling from it first, re-attempting
                // ingest. This avoids clogging up the pipeline.
                if this.ooo_buffer_rx.size_hint().0 == *this.ooo_buffer_size {
                    ready!(this.ooo_buffer_rx.as_mut().poll_next(cx))
                } else {
                    // Otherwise prefer pulling from the external stream first as freshly incoming
                    // data should be prioritized.
                    match this.stream.as_mut().poll_next(cx) {
                        Poll::Ready(Some((header, body, header_bytes))) => {
                            Some(IngestAttempt(header, body, header_bytes, 1))
                        }
                        Poll::Pending => ready!(this.ooo_buffer_rx.as_mut().poll_next(cx)),
                        Poll::Ready(None) => match this.ooo_buffer_rx.as_mut().poll_next(cx) {
                            Poll::Ready(Some(attempt)) => Some(attempt),
                            // If there's no value coming from the buffer _and_ the external stream is
                            // terminated, we can be sure nothing will come anymore.
                            Poll::Pending => None,
                            Poll::Ready(None) => None,
                        },
                    }
                }
            };
            let Some(IngestAttempt(header, body, header_bytes, counter)) = res else {
                // Both external stream and buffer stream has ended, so we stop here as well.
                return Poll::Ready(None);
            };

            // 2. Validate and check the log-integrity of the incoming operation. If it is valid it
            //    get's persisted and the log optionally pruned.
            let ingest_fut = async {
                let log_id: L = header
                    .extract()
                    .ok_or(IngestError::MissingHeaderExtension("log_id".into()))?;
                let prune_flag: PruneFlag = header
                    .extract()
                    .ok_or(IngestError::MissingHeaderExtension("prune_flag".into()))?;
                ingest_operation::<S, L, E>(
                    &mut store,
                    header,
                    body,
                    header_bytes,
                    &log_id,
                    prune_flag.is_set(),
                )
                .await
            };
            pin_mut!(ingest_fut);
            let ingest_res = ready!(ingest_fut.poll(cx));

            // 3. If the operation arrived out-of-order we can push it back into the internal
            //    buffer and try again later (attempted for a configured number of times),
            //    otherwise forward the result of ingest to the consumer.
            match ingest_res {
                Ok(IngestResult::Retry(header, body, header_bytes, num_missing)) => {
                    // The number of max. reattempts is equal the size of the buffer. As long as
                    // the buffer is just a FIFO queue it doesn't make sense to optimize over
                    // different parameters as in a worst-case distribution of items (exact
                    // reverse) this will be the max. and min. required bound.
                    if counter > *this.ooo_buffer_size {
                        return Poll::Ready(Some(Err(IngestError::MaxAttemptsReached(
                            num_missing,
                        ))));
                    }

                    // Push operation back into the internal queue, if something goes wrong here
                    // this must be an critical failure.
                    let Ok(_) = ready!(this.ooo_buffer_tx.poll_ready(cx)) else {
                        break Poll::Ready(None);
                    };

                    let Ok(_) = this.ooo_buffer_tx.start_send(IngestAttempt(
                        header,
                        body,
                        header_bytes,
                        counter + 1,
                    )) else {
                        break Poll::Ready(None);
                    };

                    continue;
                }
                Ok(IngestResult::Complete(operation)) => {
                    return Poll::Ready(Some(Ok(operation)));
                }
                Err(err) => {
                    // Ingest failed and we want the stream consumers to be aware of that.
                    return Poll::Ready(Some(Err(err)));
                }
            }
        }
    }
}

impl<St: FusedStream, S, L, E> FusedStream for Ingest<St, S, L, E>
where
    St: Stream<Item = (Header<E>, Option<Body>, Vec<u8>)>,
    S: OperationStore<L, E> + LogStore<L, E>,
    E: Extension<L> + Extension<PruneFlag> + Extensions,
{
    fn is_terminated(&self) -> bool {
        self.stream.is_terminated() && self.ooo_buffer_rx.is_terminated()
    }
}

impl<St, S, L, E> Sink<(Header<E>, Option<Body>, Vec<u8>)> for Ingest<St, S, L, E>
where
    St: Stream<Item = (Header<E>, Option<Body>, Vec<u8>)>
        + Sink<(Header<E>, Option<Body>, Vec<u8>)>,
    S: OperationStore<L, E> + LogStore<L, E>,
    E: Extension<L> + Extension<PruneFlag> + Extensions,
{
    type Error = St::Error;

    delegate_sink!(stream, (Header<E>, Option<Body>, Vec<u8>));
}

#[derive(Debug)]
struct IngestAttempt<E>(Header<E>, Option<Body>, Vec<u8>, usize);

#[cfg(test)]
mod tests {
    use futures_util::stream::iter;
    use futures_util::{StreamExt, TryStreamExt};
    use p2panda_core::{Operation, RawOperation};
    use p2panda_store::MemoryStore;

    use crate::operation::IngestError;
    use crate::stream::decode::DecodeExt;
    use crate::test_utils::{mock_stream, Extensions, StreamName};

    use super::IngestExt;

    #[tokio::test]
    async fn ingest() {
        let store = MemoryStore::<StreamName, Extensions>::new();

        let stream = mock_stream()
            .take(5)
            .decode()
            .filter_map(|item| async {
                match item {
                    Ok((header, body, header_bytes)) => Some((header, body, header_bytes)),
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
        // Reverse all items, to ingest with a worst-case out-of-order sample set.
        items.reverse();

        let stream = iter(items)
            .decode()
            .filter_map(|item| async {
                match item {
                    Ok((header, body, header_bytes)) => Some((header, body, header_bytes)),
                    Err(_) => None,
                }
            })
            // Since the sample set ordering is worst-case (fully reversed), it makes sense to keep
            // the buffer size at least as big as the sample size. Like this we can guarantee that
            // ingest (and this test) will be successful.
            .ingest(store, items_num);

        let res: Vec<Operation<Extensions>> = stream.try_collect().await.expect("not fail");
        assert_eq!(res.len(), items_num);
    }
}
