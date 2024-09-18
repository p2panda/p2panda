// SPDX-License-Identifier: AGPL-3.0-or-later

use std::pin::Pin;
use std::task::{self, Poll};

use async_stream::stream;
use futures_channel::mpsc::{self};
use futures_core::Stream;
use futures_sink::Sink;
use futures_util::stream::Fuse;
use futures_util::{select_biased, StreamExt};
use p2panda_core::extensions::DefaultExtensions;
use p2panda_core::{Body, Header, Operation};
use pin_utils::{pin_mut, unsafe_pinned};

use crate::engine::EngineError;
use crate::macros::{delegate_access_inner, delegate_sink};
use crate::operation::{decode_operation, RawOperation};

pub trait DecodeExt: Stream<Item = RawOperation> {
    fn decode(self) -> Decode<Self>
    where
        Self: Sized,
    {
        Decode::new(self)
    }
}

impl<T: ?Sized> DecodeExt for T where T: Stream<Item = RawOperation> {}

#[derive(Debug)]
#[must_use = "streams do nothing unless polled"]
pub struct Decode<St>
where
    St: Stream<Item = RawOperation>,
{
    stream: Fuse<St>,
}

impl<St> Decode<St>
where
    St: Stream<Item = RawOperation>,
{
    unsafe_pinned!(stream: Fuse<St>);

    pub(crate) fn new(stream: St) -> Decode<St> {
        Decode {
            stream: stream.fuse(),
        }
    }

    /// Acquires a reference to the underlying stream that this combinator is pulling from.
    pub fn get_ref(&self) -> &St {
        self.stream.get_ref()
    }

    /// Acquires a mutable reference to the underlying stream that this combinator is pulling from.
    ///
    /// Note that care must be taken to avoid tampering with the state of the stream which may
    /// otherwise confuse this combinator.
    pub fn get_mut(&mut self) -> &mut St {
        self.stream.get_mut()
    }

    /// Acquires a pinned mutable reference to the underlying stream that this combinator is
    /// pulling from.
    ///
    /// Note that care must be taken to avoid tampering with the state of the stream which may
    /// otherwise confuse this combinator.
    pub fn get_pin_mut(self: Pin<&mut Self>) -> Pin<&mut St> {
        self.stream().get_pin_mut()
    }

    /// Consumes this combinator, returning the underlying stream.
    ///
    /// Note that this may discard intermediate state of this combinator, so care should be taken
    /// to avoid losing resources when this is called.
    pub fn into_inner(self) -> St {
        self.stream.into_inner()
    }
}

impl<St: Stream> Stream for Decode<St> {
    type Item = Operation<_>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        loop {
            match self.as_mut().stream().poll_next(cx) {
                Poll::Ready(item) => match item {
                    // Push the item into the buffer and check whether it is full.
                    // If so, replace our buffer with a new and empty one and return
                    // the full one.
                    Some(item) => {
                        if self.items.is_empty() {
                            *self.as_mut().clock() = Some(Delay::new(self.duration));
                        }
                        self.as_mut().items().push(item);
                        if self.items.len() >= self.cap {
                            *self.as_mut().clock() = None;
                            return Poll::Ready(Some(self.as_mut().take()));
                        } else {
                            // Continue the loop
                            continue;
                        }
                    }

                    // Since the underlying stream ran out of values, return what we
                    // have buffered, if we have anything.
                    None => {
                        let last = if self.items.is_empty() {
                            None
                        } else {
                            let full_buf = mem::take(self.as_mut().items());
                            Some(full_buf)
                        };

                        return Poll::Ready(last);
                    }
                },
                // Don't return here, as we need to need check the clock.
                Poll::Pending => {}
            }

            match self
                .as_mut()
                .clock()
                .as_pin_mut()
                .map(|clock| clock.poll(cx))
            {
                Some(Poll::Ready(())) => {
                    *self.as_mut().clock() = None;
                    return Poll::Ready(Some(self.as_mut().take()));
                }
                Some(Poll::Pending) => {}
                None => {
                    debug_assert!(
                        self.items().is_empty(),
                        "Inner buffer is empty, but clock is available."
                    );
                }
            }

            return Poll::Pending;
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let chunk_len = if self.items.is_empty() { 0 } else { 1 };
        let (lower, upper) = self.stream.size_hint();
        let lower = lower.saturating_add(chunk_len);
        let upper = match upper {
            Some(x) => x.checked_add(chunk_len),
            None => None,
        };
        (lower, upper)
    }
}

// // Forwarding impl of Sink from the underlying stream
// impl<S> Sink<RawOperation> for Decode<S>
// where
//     S: Stream + Sink<RawOperation>,
// {
//     type Error = S::Error;
//
//     delegate_sink!(stream, Item);
// }

// impl Unpin for Ingest {}
//
// impl Ingest {
//     pub(super) fn new<St>(stream: St) -> Self
//     where
//         St: Stream<Item = RawOperation> + 'static,
//     {
//         let ingest_stream = stream
//             .map(|(header, body)| {
//                 decode_operation(&header, body.as_deref()).map_err(EngineError::DecodingFailed)
//             })
//             .fuse();
//
//         pin_mut!(ingest_stream);
//
//         let (_ooo_buffer_tx, mut ooo_buffer_rx) =
//             mpsc::channel::<Operation<DefaultExtensions>>(128);
//         let stream = stream! {
//             loop {
//                 select_biased! {
//                     operation = ingest_stream.next() => {
//                         if let Some(operation) = operation {
//                             yield operation;
//                         }
//                     },
//                     operation = ooo_buffer_rx.next() => {
//                         if let Some(operation) = operation {
//                             yield Ok((operation.header, operation.body));
//                         }
//                     },
//                 }
//             }
//         };
//
//         Self {
//             stream: Box::pin(stream),
//         }
//     }
//
//     // delegate_access_inner!(stream, St, ());
// }
//
// // impl FusedStream for Ingest {
// //     fn is_terminated(&self) -> bool {
// //         self.stream.is_terminated()
// //     }
// // }
//
// impl Stream for Ingest {
//     type Item = Result<(Header<DefaultExtensions>, Option<Body>), EngineError>;
//
//     fn poll_next(mut self: Pin<&mut Self>, cx: &mut task::Context<'_>) -> Poll<Option<Self::Item>> {
//         self.stream.poll_next_unpin(cx)
//     }
// }
//
// // impl Sink<RawOperation> for Ingest {
// //     type Error = EngineError;
// //
// //     fn poll_ready(
// //         mut self: Pin<&mut Self>,
// //         cx: &mut task::Context<'_>,
// //     ) -> Poll<Result<(), Self::Error>> {
// //         self.poll_ready(cx)
// //             .map_err(|_| EngineError::IngestPipeFailed)
// //     }
// //
// //     fn start_send(mut self: Pin<&mut Self>, item: RawOperation) -> Result<(), Self::Error> {
// //         self.ingest_buffer_tx
// //             .start_send_unpin(item)
// //             .map_err(|_| EngineError::IngestPipeFailed)
// //     }
// //
// //     fn poll_flush(
// //         mut self: Pin<&mut Self>,
// //         cx: &mut task::Context<'_>,
// //     ) -> Poll<Result<(), Self::Error>> {
// //         self.ingest_buffer_tx
// //             .poll_flush_unpin(cx)
// //             .map_err(|_| EngineError::IngestPipeFailed)
// //     }
// //
// //     fn poll_close(
// //         mut self: Pin<&mut Self>,
// //         cx: &mut task::Context<'_>,
// //     ) -> Poll<Result<(), Self::Error>> {
// //         self.ingest_buffer_tx
// //             .poll_close_unpin(cx)
// //             .map_err(|_| EngineError::IngestPipeFailed)
// //     }
// // }
//
// #[cfg(test)]
// mod tests {
//     use futures_util::StreamExt;
//
//     use crate::{test_utils::mock_stream, EngineExt};
//
//     #[tokio::test]
//     async fn as_much_out_than_in() {
//         let local = tokio::task::LocalSet::new();
//
//         local
//             .run_until(async {
//                 let mut stream = mock_stream().take(5).ingest();
//
//                 while let Some(item) = stream.next().await {
//                     println!("{:?}", item);
//                 }
//
//                 // assert_eq!(stream.count().await, 5);
//             })
//             .await;
//     }
// }
