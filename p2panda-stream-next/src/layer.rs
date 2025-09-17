// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::VecDeque;
use std::marker::PhantomData;
use std::pin::Pin;
use std::task::{Context, Poll};

use futures_core::future::BoxFuture;
use futures_core::{Stream, ready};
use pin_project::pin_project;
use tokio::task::JoinHandle;

/// Interface for implementing data processing layers.
pub trait Layer<T>
where
    Self: Send + Sync + 'static,
    Self::Output: Send + 'static,
    Self::Error: Send + 'static,
{
    type Output;

    type Error;

    /// Consumes an item for further processing.
    fn process(&self, input: T) -> BoxFuture<'_, Result<(), Self::Error>>;

    /// Returns future with processed output or `None` if processor stopped.
    fn next(&self) -> BoxFuture<'_, Result<Option<Self::Output>, Self::Error>>;
}

/// Extension trait for `Layer` that provides convenient methods for s tream processing.
pub trait LayerExt<T>: Layer<T> {
    /// Converts this Layer into a Stream that yields the Layer's output items.
    ///
    /// The resulting stream will continuously poll the layer's `next()` method and yield items as
    /// they become available.
    fn into_stream(self) -> LayerStream<Self, T>
    where
        Self: Sized,
    {
        LayerStream::new(self)
    }

    /// Processes items from the provided stream through this layer.
    fn process_stream<S>(self, input_stream: S) -> ChainedLayerStream<Self, S, T>
    where
        Self: Sized,
        S: Stream<Item = T>,
    {
        ChainedLayerStream::new(self, input_stream)
    }
}

impl<L, T> LayerExt<T> for L where L: Layer<T> {}

/// Stream for the [`into_stream`](LayerExt::into_stream) method.
#[pin_project]
#[must_use = "streams do nothing unless polled"]
pub struct LayerStream<L, T> {
    #[pin]
    layer: L,
    _marker: PhantomData<T>,
}

impl<L, T> LayerStream<L, T>
where
    L: Layer<T>,
{
    pub(crate) fn new(layer: L) -> LayerStream<L, T> {
        LayerStream {
            layer,
            _marker: PhantomData,
        }
    }

    /// Returns reference to the underlying layer.
    pub fn get_ref(&self) -> &L {
        &self.layer
    }

    /// Returns mutable reference to the underlying layer.
    pub fn get_mut(&mut self) -> &mut L {
        &mut self.layer
    }

    /// Consume this stream and return the underlying layer.
    pub fn into_inner(self) -> L {
        self.layer
    }
}

pub struct BufferedLayer<L, T>
where
    L: Layer<T>,
    T: Send + Sync + 'static,
{
    process_tx: flume::Sender<T>,
    output_rx: flume::Receiver<Result<L::Output, L::Error>>,
    _handle: JoinHandle<()>,
    _marker: PhantomData<(L, T)>,
}

impl<L, T> BufferedLayer<L, T>
where
    L: Layer<T> + Send + 'static,
    T: Send + Sync + 'static,
    L::Output: Send + 'static,
    L::Error: Send + 'static,
{
    pub fn new(layer: L, buffer_size: usize) -> Self {
        let (process_tx, process_rx) = flume::bounded(buffer_size);
        let (output_tx, output_rx) = flume::bounded(buffer_size);

        let worker_handle = tokio::spawn(async move {
            loop {
                tokio::select! {
                    result = process_rx.recv_async() => {
                        match result {
                            Ok(input) => {
                                if let Err(err) = layer.process(input).await {
                                    let _ = output_tx.send_async(Err(err)).await;
                                }
                            },
                            Err(_) => {
                                // Channel closed, end here.
                                break
                            }
                        }
                    }
                    result = layer.next() => {
                        match result {
                            Ok(Some(output)) => {
                                let _ = output_tx.send_async(Ok(output)).await;
                            }
                            Ok(None) => {
                                // Processor seized work, end here.
                                break;
                            }
                            Err(err) => {
                                let _ = output_tx.send_async(Err(err)).await;
                            }
                        }
                    }
                }
            }
        });

        Self {
            process_tx,
            output_rx,
            _handle: worker_handle,
            _marker: PhantomData,
        }
    }
}

impl<L, T> Layer<T> for BufferedLayer<L, T>
where
    // "Inner"-Layer - wrapped by this impl
    L: Layer<T>,
    T: Send + Sync + 'static,
{
    type Output = L::Output;

    type Error = L::Error;

    fn process(&self, input: T) -> BoxFuture<'_, Result<(), Self::Error>> {
        Box::pin(async {
            // @TODO: It should be fine to ignore the error here? Otherwise we need to wrap L::Error
            // with an BufferedLayerError type.
            let _ = self.process_tx.send_async(input).await;
            Ok(())
        })
    }

    fn next(&self) -> BoxFuture<'_, Result<Option<Self::Output>, Self::Error>> {
        Box::pin(async {
            match self.output_rx.recv_async().await {
                Ok(output) => output.map(Some),
                Err(_) => Ok(None), // Channel closed, no more items.
            }
        })
    }
}

#[pin_project]
#[must_use = "streams do nothing unless polled"]
pub struct ChainedLayerStream<L, S, T>
where
    L: Layer<T>,
    S: Stream<Item = T>,
{
    #[pin]
    layer: L,
    #[pin]
    input_stream: S,
    pending_outputs: VecDeque<L::Output>,
    _marker: PhantomData<T>,
}

impl<L, S, T> ChainedLayerStream<L, S, T>
where
    L: Layer<T>,
    S: Stream<Item = T>,
{
    pub(crate) fn new(layer: L, input_stream: S) -> ChainedLayerStream<L, S, T> {
        ChainedLayerStream {
            layer,
            input_stream,
            pending_outputs: VecDeque::new(),
            _marker: PhantomData,
        }
    }

    /// Returns reference to the underlying layer.
    pub fn get_ref(&self) -> &L {
        &self.layer
    }

    /// Returns mutable reference to the underlying layer.
    pub fn get_mut(&mut self) -> &mut L {
        &mut self.layer
    }

    /// Returns reference to the underlying input stream.
    pub fn get_input_ref(&self) -> &S {
        &self.input_stream
    }

    /// Consume this stream and return the underlying layer and input stream.
    pub fn into_inner(self) -> (L, S) {
        (self.layer, self.input_stream)
    }
}

impl<L, S, T> Stream for ChainedLayerStream<L, S, T>
where
    L: Layer<T>,
    S: Stream<Item = T>,
{
    type Item = Result<L::Output, L::Error>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut this = self.project();

        loop {
            // 1. Try to get any buffered outputs from the layer.
            if let Some(output) = this.pending_outputs.pop_front() {
                return Poll::Ready(Some(Ok(output)));
            }

            // 2. Try to get the next item from the input stream to process.
            match ready!(this.input_stream.as_mut().poll_next(cx)) {
                Some(item) => {
                    let mut process_fut = this.layer.process(item);

                    match ready!(process_fut.as_mut().poll(cx)) {
                        Ok(()) => {
                            // After processing, try to get outputs from the layer.
                            let mut next_fut = this.layer.next();

                            match ready!(next_fut.as_mut().poll(cx)) {
                                Ok(Some(output)) => {
                                    return Poll::Ready(Some(Ok(output)));
                                }
                                Ok(None) => {
                                    // No output available, continue to next input.
                                    continue;
                                }
                                Err(error) => return Poll::Ready(Some(Err(error))),
                            }
                        }
                        Err(error) => return Poll::Ready(Some(Err(error))),
                    }
                }
                None => {
                    // Input stream is exhausted, try to get any remaining outputs.
                    let mut next_fut = this.layer.next();

                    match ready!(next_fut.as_mut().poll(cx)) {
                        Ok(Some(output)) => return Poll::Ready(Some(Ok(output))),
                        Ok(None) => return Poll::Ready(None),
                        Err(error) => return Poll::Ready(Some(Err(error))),
                    }
                }
            }
        }
    }
}
