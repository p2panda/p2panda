// SPDX-License-Identifier: MIT OR Apache-2.0

use std::pin::Pin;
use std::task::{Context, Poll};

use futures_core::Stream;
use pin_project::pin_project;

use crate::processors::Processor;
use crate::processors::buffered::{Buffer, BufferReceiver, BufferSender};

/// Extension trait for `Processor` that provides convenient methods for `Stream` processing.
pub trait ProcessorExt<T>: Processor<T>
where
    Self: 'static,
    T: 'static,
{
    /// Converts this processor into a `Stream` that takes an input stream and yields the
    /// processor's output items.
    fn into_stream<S: Stream<Item = T>>(self, input_stream: S) -> ProcessorStream<Self, S, T>
    where
        Self: Sized,
    {
        ProcessorStream::new(self, input_stream)
    }
}

impl<P, T: 'static> ProcessorExt<T> for P where P: Processor<T> + 'static {}

#[pin_project]
#[must_use = "streams do nothing unless polled"]
pub struct ProcessorStream<P, S, T>
where
    P: Processor<T>,
    S: Stream<Item = T>,
{
    #[pin]
    input_stream: S,
    buffer: Buffer,
    tx: BufferSender<T>,
    rx: BufferReceiver<P, T>,
}

impl<P, S, T> ProcessorStream<P, S, T>
where
    P: Processor<T> + 'static,
    S: Stream<Item = T>,
    T: 'static,
{
    pub(crate) fn new(processor: P, input_stream: S) -> ProcessorStream<P, S, T> {
        // Wrap processor around an unbounded buffer making sure we're correctly awaiting the
        // futures returned by "process" and "next", correctly handling processors which might have
        // "expensive" async behavior.
        let (buffer, tx, rx) = Buffer::new(processor);
        ProcessorStream {
            input_stream,
            buffer,
            tx,
            rx,
        }
    }
}

impl<P, S, T> Stream for ProcessorStream<P, S, T>
where
    P: Processor<T>,
    S: Stream<Item = T>,
{
    type Item = Result<P::Output, P::Error>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut this = self.project();
        let mut made_progress = false;

        // Check input stream.
        //
        // If there's an item in the previous `Stream`, we take it and push it into our internal
        // buffer queue. If there's nothing in it (Poll::Pending), we continue.
        //
        // If the input stream terminated, we ignore this and continue as well since we don't know
        // if the processor has more work to do. Processors never cease operation and are only
        // stopped via higher level logic. This is why we also continue here in the `Stream` and
        // never terminate.
        match this.input_stream.as_mut().poll_next(cx) {
            Poll::Ready(Some(input)) => {
                let _ = this.tx.send(input);
                made_progress = true;
            }
            Poll::Ready(None) | Poll::Pending => (),
        };

        // Check output of processor.
        //
        // If an output item was given, we yield it in this Stream. If an error was returned we do
        // the same.
        //
        // If the next method returns a pending state we assume this is because there are no items
        // given.
        match this.rx.poll_recv(cx) {
            Poll::Ready(item) => Poll::Ready(item),
            Poll::Pending => {
                if made_progress {
                    // We sent new input, try again immediately.
                    cx.waker().wake_by_ref();
                }
                Poll::Pending
            }
        }
    }
}

/// Extension trait for `Stream` that provides convenient methods to layer up processors.
pub trait StreamLayerExt<T>: Stream<Item = T> + Sized
where
    T: 'static,
{
    /// Processes this stream through the provided layer.
    fn layer<P>(self, processor: P) -> ProcessorStream<P, Self, T>
    where
        P: Processor<T> + 'static,
    {
        ProcessorStream::new(processor, self)
    }
}

impl<S, T: 'static> StreamLayerExt<T> for S where S: Stream<Item = T> {}
