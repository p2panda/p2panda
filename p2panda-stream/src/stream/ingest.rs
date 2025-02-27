// SPDX-License-Identifier: MIT OR Apache-2.0

use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;

use futures_channel::mpsc::{self};
use futures_util::stream::{Fuse, FusedStream};
use futures_util::task::{Context, Poll};
use futures_util::{FutureExt, Sink, Stream, StreamExt, ready};
use p2panda_core::prune::PruneFlag;
use p2panda_core::{Body, Extension, Extensions, Header, Operation};
use p2panda_store::{LogStore, OperationStore};
use pin_project::pin_project;

use crate::macros::{delegate_access_inner, delegate_sink};
use crate::operation::{IngestError, IngestResult, ingest_operation};

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
    ingest_fut: Option<Pin<IngestFut<E>>>,
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
            ingest_fut: None,
            _marker: PhantomData,
        }
    }

    delegate_access_inner!(stream, St, (.));
}

impl<St, S, L, E> Stream for Ingest<St, S, L, E>
where
    St: Stream<Item = (Header<E>, Option<Body>, Vec<u8>)>,
    S: OperationStore<L, E> + LogStore<L, E> + 'static,
    E: Extension<L> + Extension<PruneFlag> + Extensions + Send + Sync + 'static,
    L: Send + Sync,
{
    type Item = Result<Operation<E>, IngestError>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut this = self.project();
        let mut park_buffer = false;

        loop {
            // 1. Attempt validating and ingesting operation (this is a future we need to poll
            //    a while until it resolves).
            if let Some(ingest_fut) = this.ingest_fut.as_mut() {
                let ingest_res = ready!(ingest_fut.poll_unpin(cx));
                this.ingest_fut.take();

                // 2. If the operation arrived out-of-order we can push it back into the internal
                //    buffer and try again later (attempted for a configured number of times),
                //    otherwise forward the result to the stream consumer.
                match ingest_res {
                    Ok((IngestResult::Retry(header, body, header_bytes, num_missing), counter)) => {
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

                        // In the next iteration we should prioritize the stream again.
                        park_buffer = true;

                        continue;
                    }
                    Ok((IngestResult::Complete(operation), _)) => {
                        return Poll::Ready(Some(Ok(operation)));
                    }
                    Err(err) => {
                        // Ingest failed and we want the stream consumers to be aware of that.
                        return Poll::Ready(Some(Err(err)));
                    }
                }
            }

            // 3. Pull in the next item from the external stream or out-of-order buffer.
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
                        Poll::Pending => {
                            // If we're getting back to the buffer queue after a failed ingest
                            // attempt, we should "park" here instead and allow the runtime to try
                            // polling the stream again next time.
                            //
                            // Otherwise we run into a loop where the runtime will never have the
                            // chance again to take in new operations and we end up exhausting our
                            // re-attempt counter for no reason.
                            if park_buffer {
                                return Poll::Pending;
                            }
                            ready!(this.ooo_buffer_rx.as_mut().poll_next(cx))
                        }
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

            // 4. Validate and check the log-integrity of the incoming operation. If it is valid it
            //    get's persisted and the log optionally pruned.
            let mut store = this.store.clone();

            let ingest_fut = async move {
                let log_id = header
                    .extension()
                    .ok_or(IngestError::MissingHeaderExtension("log_id".into()))?;
                let prune_flag: PruneFlag = header
                    .extension()
                    .ok_or(IngestError::MissingHeaderExtension("prune_flag".into()))?;

                let ingest_res = ingest_operation::<S, L, E>(
                    &mut store,
                    header,
                    body,
                    header_bytes,
                    &log_id,
                    prune_flag.is_set(),
                )
                .await;

                ingest_res.map(|res| (res, counter))
            };

            this.ingest_fut.replace(Box::pin(ingest_fut));
        }
    }
}

impl<St: FusedStream, S, L, E> FusedStream for Ingest<St, S, L, E>
where
    St: Stream<Item = (Header<E>, Option<Body>, Vec<u8>)>,
    S: OperationStore<L, E> + LogStore<L, E> + 'static,
    E: Extension<L> + Extension<PruneFlag> + Extensions + Send + Sync + 'static,
    L: Send + Sync,
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

type AttemptCounter = usize;

type IngestFut<E> =
    Box<dyn Future<Output = Result<(IngestResult<E>, AttemptCounter), IngestError>> + Send>;

#[derive(Debug)]
struct IngestAttempt<E>(Header<E>, Option<Body>, Vec<u8>, AttemptCounter);

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use futures_util::stream::iter;
    use futures_util::{StreamExt, TryStreamExt};
    use p2panda_core::{Operation, RawOperation};
    use p2panda_store::MemoryStore;
    use p2panda_store::sqlite::store::SqliteStore;
    use p2panda_store::sqlite::test_utils::initialize_sqlite_db;
    use tokio::sync::mpsc;
    use tokio::time;
    use tokio_stream::wrappers::ReceiverStream;

    use crate::operation::IngestError;
    use crate::stream::decode::DecodeExt;
    use crate::test_utils::{Extensions, StreamName, mock_stream};

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

    #[tokio::test]
    async fn ingest_async_store_bug() {
        // Related issue: https://github.com/p2panda/p2panda/issues/694
        let pool = initialize_sqlite_db().await;
        let store = SqliteStore::<StreamName, Extensions>::new(pool);
        let stream = mock_stream()
            .take(5)
            .decode()
            .filter_map(|item| async { item.ok() })
            .ingest(store, 16);
        let res: Vec<Operation<Extensions>> = stream.try_collect().await.expect("no fail");
        assert_eq!(res.len(), 5);
    }

    #[tokio::test]
    async fn exhaust_re_attempts_too_early_bug() {
        // Related issue: https://github.com/p2panda/p2panda/issues/665
        let store = MemoryStore::<StreamName, Extensions>::new();
        let (tx, rx) = mpsc::channel::<RawOperation>(10);

        // Incoming operations in order: 1, 2, 3, 4, 5, 6, 7, 8, 9, 0 (<-- first operation in log
        // comes last in).
        let mut operations: Vec<RawOperation> = mock_stream().take(10).collect().await;
        operations.rotate_left(1);

        tokio::spawn(async move {
            // Reverse operations and pop one after another from the back to ingest.
            operations.reverse();
            while let Some(operation) = operations.pop() {
                let _ = tx.send(operation).await;

                // Waiting here is crucial to cause the bug: The polling logic will not receive a
                // new item directly from the stream but rather prioritize the buffer.
                time::sleep(Duration::from_millis(10)).await;
            }
        });

        let stream = ReceiverStream::new(rx)
            .decode()
            .filter_map(|item| async {
                match item {
                    Ok((header, body, header_bytes)) => Some((header, body, header_bytes)),
                    Err(_) => None,
                }
            })
            .ingest(store, 128); // out-of-order buffer is large enough

        let res: Vec<Operation<Extensions>> = stream.try_collect().await.expect("not fail");
        assert_eq!(res.len(), 10);
    }
}
