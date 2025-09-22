// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::VecDeque;
use std::marker::PhantomData;
use std::pin::Pin;
use std::task::{Context, Poll};

use futures_core::Stream;
use pin_project::pin_project;
use tokio::pin;

use crate::processors::Processor;

/// Extension trait for `Processor` that provides convenient methods for `Stream` processing.
pub trait ProcessorExt<T>: Processor<T> {
    /// Converts this processor into a `Stream` that takes an input stream and yields the
    /// processor's output items.
    fn into_stream<S: Stream<Item = T>>(self, input_stream: S) -> ProcessorStream<Self, S, T>
    where
        Self: Sized,
    {
        ProcessorStream::new(self, input_stream)
    }
}

impl<P, T> ProcessorExt<T> for P where P: Processor<T> {}

#[pin_project]
#[must_use = "streams do nothing unless polled"]
pub struct ProcessorStream<P, S, T>
where
    P: Processor<T>,
    S: Stream<Item = T>,
{
    #[pin]
    processor: P,
    #[pin]
    input_stream: S,
    inputs: VecDeque<T>,
    outputs: VecDeque<P::Output>,
    _marker: PhantomData<T>,
}

impl<P, S, T> ProcessorStream<P, S, T>
where
    P: Processor<T>,
    S: Stream<Item = T>,
{
    pub(crate) fn new(processor: P, input_stream: S) -> ProcessorStream<P, S, T> {
        ProcessorStream {
            processor,
            input_stream,
            inputs: VecDeque::new(),
            outputs: VecDeque::new(),
            _marker: PhantomData,
        }
    }
}

#[derive(Default)]
enum ProcessorStreamState {
    #[default]
    InputStream,
    Process,
    Next,
}

impl ProcessorStreamState {
    fn next(self) -> Self {
        match self {
            ProcessorStreamState::InputStream => ProcessorStreamState::Process,
            ProcessorStreamState::Process => ProcessorStreamState::Next,
            ProcessorStreamState::Next => unreachable!(),
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
        let mut state = ProcessorStreamState::default();

        loop {
            match state {
                ProcessorStreamState::InputStream => {
                    // 1. Check input stream.
                    //
                    // If there's an item in the previous `Stream`, we take it and push it into our
                    // internal queue. If there's nothing in it (Poll::Pending), we go into the
                    // next state.
                    //
                    // If the input stream terminated, we ignore this and go to the next state as
                    // well since we don't know if the processor has more work to do. Processors
                    // never seize operation and are only stopped via higher level logic. This is
                    // why we also continue here in the `Stream` and never terminate.
                    match this.input_stream.as_mut().poll_next(cx) {
                        Poll::Ready(Some(input)) => {
                            this.inputs.push_back(input);
                        }
                        Poll::Ready(None) | Poll::Pending => {
                            state = state.next();
                        }
                    }
                }
                ProcessorStreamState::Process => {
                    // 2. Check input queue for items we can forward to processor.
                    //
                    // If there's any items we can give to the processor, we call it's "process"
                    // method with it.
                    //
                    // If no error was returned, we continue going into the next state. If an error
                    // occurred we yield it in this stream.
                    //
                    // If polling the "process" method ever returns Poll::Pending we panic (!). Our
                    // implementation is rather simplified and never parks any to-be-polled futures
                    // somewhere (since moving the future out of the processor is tricky with
                    // ownership rules). Instead we expect every processor implementation to always
                    // return instantly and if _not_ it needs to come with it's own buffering
                    // logic. Implementers can use the BufferedProcessor wrapper which does exactly
                    // that job and is safely to use with this.
                    if let Some(input) = this.inputs.pop_front() {
                        let fut = this.processor.process(input);
                        pin!(fut);

                        match fut.as_mut().poll(cx) {
                            Poll::Ready(Ok(())) => (),
                            Poll::Ready(Err(err)) => return Poll::Ready(Some(Err(err))),
                            Poll::Pending => panic!("unsound behaviour, use buffered processor"),
                        }
                    }

                    state = state.next();
                }
                ProcessorStreamState::Next => {
                    // 3. Check "next" method on processor to see if there's any output.
                    //
                    // If an output item was given, we yield it in this Stream. If an error was
                    // returned we do the same.
                    //
                    // If the next method returns a pending state we assume this is because there
                    // are no items given.
                    //
                    // From here on we _dont_ go into the next state but leave the loop and hope
                    // that this very stream gets waken up again in the future by the runtime. Then
                    // we start again from the top! Yay.
                    let fut = this.processor.next();
                    pin!(fut);

                    match fut.as_mut().poll(cx) {
                        Poll::Ready(Ok(output)) => return Poll::Ready(Some(Ok(output))),
                        Poll::Ready(Err(err)) => return Poll::Ready(Some(Err(err))),
                        Poll::Pending => {
                            return Poll::Pending;
                        }
                    }
                }
            }
        }
    }
}

/// Extension trait for `Stream` that provides convenient methods to layer up processors.
pub trait StreamLayerExt<T>: Stream<Item = T> + Sized {
    /// Processes this stream through the provided layer.
    fn layer<P>(self, processor: P) -> ProcessorStream<P, Self, T>
    where
        P: Processor<T>,
    {
        ProcessorStream::new(processor, self)
    }
}

impl<S, T> StreamLayerExt<T> for S where S: Stream<Item = T> {}
