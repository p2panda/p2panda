// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::VecDeque;
use std::marker::PhantomData;
use std::pin::Pin;
use std::task::{Context, Poll};

use futures_core::{Stream, ready};
use futures_util::FutureExt;
use pin_project::pin_project;
use tokio::task::JoinHandle;
use tokio::{pin, task};

/// Interface for implementing data processing layers.
pub trait Layer<T> {
    type Output;

    type Error;

    /// Consumes an item for further processing.
    fn process(&self, input: T) -> impl Future<Output = Result<(), Self::Error>>;

    /// Returns future with processed output or `None` if processor stopped.
    fn next(&self) -> impl Future<Output = Result<Option<Self::Output>, Self::Error>>;
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
}

impl<L, T> LayerExt<T> for L where L: Layer<T> {}

/// Extension trait for `Stream` that provides convenient methods for processing with layers.
pub trait StreamChainExt<T>: Stream<Item = T> + Sized {
    /// Processes this stream through the provided layer.
    fn process_with<L>(self, layer: L) -> ChainedLayerStream<L, Self, T>
    where
        L: Layer<T>,
    {
        ChainedLayerStream::new(layer, self)
    }
}

impl<S, T> StreamChainExt<T> for S where S: Stream<Item = T> {}

/// Stream for the [`into_stream`](LayerExt::into_stream) method.
#[must_use = "streams do nothing unless polled"]
pub struct LayerStream<L, T> {
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

impl<L, T> Stream for LayerStream<L, T>
where
    L: Layer<T>,
{
    type Item = Result<L::Output, L::Error>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let next_fut = self.layer.next();
        pin!(next_fut);

        match ready!(next_fut.poll_unpin(cx)) {
            Ok(Some(output)) => Poll::Ready(Some(Ok(output))),
            Ok(None) => Poll::Ready(None),
            Err(error) => Poll::Ready(Some(Err(error))),
        }
    }
}

pub struct BufferedLayer<L, T>
where
    L: Layer<T>,
{
    process_tx: async_channel::Sender<T>,
    output_rx: async_channel::Receiver<Result<L::Output, L::Error>>,
    _handle: JoinHandle<()>,
    _marker: PhantomData<(L, T)>,
}

impl<L, T> BufferedLayer<L, T>
where
    L: Layer<T> + 'static,
    T: 'static,
{
    pub fn new(layer: L, buffer_size: usize) -> Self {
        let (process_tx, process_rx) = async_channel::bounded(buffer_size);
        let (output_tx, output_rx) = async_channel::bounded(buffer_size);

        let worker_handle = task::spawn_local(async move {
            loop {
                tokio::select! {
                    result = process_rx.recv() => {
                        match result {
                            Ok(input) => {
                                if let Err(err) = layer.process(input).await {
                                    let _ = output_tx.send(Err(err)).await;
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
                                let _ = output_tx.send(Ok(output)).await;
                            }
                            Ok(None) => {
                                // Processor seized work, end here.
                                break;
                            }
                            Err(err) => {
                                let _ = output_tx.send(Err(err)).await;
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
{
    type Output = L::Output;

    type Error = L::Error;

    async fn process(&self, input: T) -> Result<(), Self::Error> {
        // @TODO: It should be fine to ignore the error here? Otherwise we need to wrap L::Error
        // with an BufferedLayerError type.
        let _ = self.process_tx.send(input).await;
        Ok(())
    }

    async fn next(&self) -> Result<Option<Self::Output>, Self::Error> {
        match self.output_rx.recv().await {
            Ok(output) => output.map(Some),
            Err(_) => Ok(None), // Channel closed, no more items.
        }
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
                    let process_fut = this.layer.process(item);

                    pin!(process_fut);

                    match ready!(process_fut.as_mut().poll(cx)) {
                        Ok(()) => {
                            // After processing, try to get outputs from the layer.
                            let next_fut = this.layer.next();

                            pin!(next_fut);

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
                    let next_fut = this.layer.next();

                    pin!(next_fut);

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

#[cfg(test)]
mod tests {
    use std::convert::Infallible;
    use std::sync::atomic::AtomicU64;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use futures_test::task::noop_context;
    use futures_util::{FutureExt, StreamExt, stream};
    use tokio::{pin, time};

    use super::*;

    /// Layer turning all strings into UPPERCASE.
    #[derive(Default)]
    struct UppercaseLayer {
        outputs: Arc<Mutex<VecDeque<String>>>,
    }

    impl Layer<String> for UppercaseLayer {
        type Output = String;

        type Error = Infallible;

        async fn process(&self, input: String) -> Result<(), Self::Error> {
            self.outputs.lock().unwrap().push_back(input.to_uppercase());
            Ok(())
        }

        async fn next(&self) -> Result<Option<Self::Output>, Self::Error> {
            Ok(self.outputs.lock().unwrap().pop_front())
        }
    }

    /// Layer adding a counter to any item.
    #[derive(Default)]
    struct CounterLayer<T> {
        outputs: Arc<tokio::sync::Mutex<VecDeque<WithCounter<T>>>>,
        counter: AtomicU64,
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct WithCounter<T> {
        item: T,
        counter: u64,
    }

    impl<T> Layer<T> for CounterLayer<T>
    where
        T: 'static,
    {
        type Output = WithCounter<T>;

        type Error = String;

        async fn process(&self, item: T) -> Result<(), Self::Error> {
            let counter = self
                .counter
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

            self.outputs
                .lock()
                .await
                .push_back(WithCounter { item, counter });

            Ok(())
        }

        async fn next(&self) -> Result<Option<Self::Output>, Self::Error> {
            Ok(self.outputs.lock().await.pop_front())
        }
    }

    /// Test layer simulating "expensive" async operations when calling "next" or "process".
    #[derive(Clone)]
    struct SlowLayer {
        process_delay: Duration,
        next_delay: Duration,
        processed_items: Arc<Mutex<Vec<usize>>>,
        output_queue: Arc<Mutex<VecDeque<String>>>,
        should_error: bool,
    }

    impl SlowLayer {
        fn new() -> Self {
            Self {
                process_delay: Duration::from_millis(0),
                next_delay: Duration::from_millis(0),
                processed_items: Arc::new(Mutex::new(Vec::new())),
                output_queue: Arc::new(Mutex::new(VecDeque::new())),
                should_error: false,
            }
        }

        fn with_process_delay(mut self, process_delay: Duration) -> Self {
            self.process_delay = process_delay;
            self
        }

        fn with_next_delay(mut self, next_delay: Duration) -> Self {
            self.next_delay = next_delay;
            self
        }

        fn with_error_mode(mut self) -> Self {
            self.should_error = true;
            self
        }
    }

    impl Layer<usize> for SlowLayer {
        type Output = String;

        type Error = String;

        async fn process(&self, input: usize) -> Result<(), Self::Error> {
            time::sleep(self.process_delay).await;

            if self.should_error {
                return Err(format!("error in process method: {}", input));
            }

            self.processed_items.lock().unwrap().push(input);
            self.output_queue
                .lock()
                .unwrap()
                .push_back(format!("processed_{}", input));

            Ok(())
        }

        async fn next(&self) -> Result<Option<Self::Output>, Self::Error> {
            time::sleep(self.next_delay).await;

            if self.should_error {
                return Err("error in next method".to_string());
            }

            Ok(self.output_queue.lock().unwrap().pop_front())
        }
    }

    #[tokio::test]
    async fn process_and_next_semantics() {
        // 1. Regular Layer implementations do not follow the Stream semantics and terminate
        //    operation when there's no input anymore. We can process and call next as soon as
        //    there is work being done.
        let uppercase = UppercaseLayer::default();
        uppercase.process("Hello".to_string()).await.unwrap();
        assert_eq!(uppercase.next().await, Ok(Some("HELLO".to_string())));
        assert_eq!(uppercase.next().await, Ok(None)); // No work being done right now

        // Continue doing new work ..
        uppercase.process("World".to_string()).await.unwrap();
        assert_eq!(uppercase.next().await, Ok(Some("WORLD".to_string())));
        assert_eq!(uppercase.next().await, Ok(None)); // No work again

        // Poll will return a value, we haven't terminated anything.
        // let mut cx = noop_context();
        // assert_eq!(uppercase.next().poll_unpin(&mut cx), Poll::Ready(Ok(None)));

        // 2. While when turning it into a Stream we follow it's semantics: When the input stream
        //    seizes (by returning Poll::Ready(None)), all chained streams will forward that
        //    termination.
        let mut uppercase_stream =
            stream::iter(vec!["Good".to_string(), "Bye!".to_string()]).process_with(uppercase);
        assert_eq!(uppercase_stream.next().await, Some(Ok("GOOD".to_string())));
        assert_eq!(uppercase_stream.next().await, Some(Ok("BYE!".to_string())));

        // Input stream seized, the stream layer terminated.
        assert_eq!(uppercase_stream.next().await, None);

        // Polling will return Poll::Ready(None), which means stream termination.
        let mut cx = noop_context();
        assert_eq!(uppercase_stream.poll_next_unpin(&mut cx), Poll::Ready(None));
    }

    #[tokio::test]
    async fn chaining_layers() {
        let uppercase = UppercaseLayer::default();
        let counter = CounterLayer::<String>::default();

        let input_stream = stream::iter(vec![
            "im".to_string(),
            "very".to_string(),
            "silent".to_string(),
        ]);

        let stream = input_stream
            .process_with(uppercase)
            // @TODO: A nice out-of-the-box error handling solution could be handy here.
            .filter_map(|result| async {
                match result {
                    Ok(item) => Some(item),
                    Err(_) => panic!("should not fail"),
                }
            })
            .process_with(counter);

        pin!(stream);

        assert_eq!(
            stream.next().await.unwrap(),
            Ok(WithCounter {
                item: "IM".to_string(),
                counter: 0,
            }),
        );

        assert_eq!(
            stream.next().await.unwrap(),
            Ok(WithCounter {
                item: "VERY".to_string(),
                counter: 1,
            }),
        );

        assert_eq!(
            stream.next().await.unwrap(),
            Ok(WithCounter {
                item: "SILENT".to_string(),
                counter: 2,
            }),
        );
    }

    #[tokio::test]
    async fn buffered_layer_polling() {
        let local = task::LocalSet::new();

        local
            .run_until(async move {
                // Have a "slow" processing layer which will not return a result instantly (first poll will
                // not yield Poll::Ready(T).
                let slow_layer = SlowLayer::new()
                    .with_process_delay(Duration::from_millis(100))
                    .with_next_delay(Duration::from_millis(50));

                let mut cx = noop_context();
                assert_eq!(
                    {
                        let fut = slow_layer.process(1);
                        pin!(fut);
                        fut.poll_unpin(&mut cx)
                    },
                    Poll::Pending
                );
                assert_eq!(
                    slow_layer.clone().into_stream().poll_next_unpin(&mut cx),
                    Poll::Pending
                );

                // We need to buffer the result, either through implementing it as part of the layer or by
                // wrapping it with BufferedLayer, otherwise we would hang forever always re-polling a
                // never-resolving Future when combined with our (simple) Stream implementation.
                let buffered_layer = BufferedLayer::new(slow_layer, 10);

                assert_eq!(
                    {
                        let fut = buffered_layer.process(1);
                        pin!(fut);
                        let mut cx = noop_context();
                        fut.poll_unpin(&mut cx)
                    },
                    Poll::Ready(Ok(()))
                );
                assert_eq!(
                    {
                        let stream = buffered_layer.into_stream();
                        pin!(stream);
                        let mut cx = noop_context();
                        stream.poll_next(&mut cx)
                    },
                    Poll::Pending
                );
            })
            .await;
    }

    #[tokio::test]
    async fn error_handling() {
        let local = task::LocalSet::new();

        local
            .run_until(async move {
                let slow_layer = SlowLayer::new().with_error_mode();
                // @TODO: Layer implementations return the error on "process" while BufferedLayer only on
                // the "next" call?
                assert!(slow_layer.process(1).await.is_err());
                assert!(slow_layer.next().await.is_err());

                let buffered = BufferedLayer::new(slow_layer, 10);
                assert!(buffered.process(1).await.is_ok());
                assert!(buffered.next().await.is_err());
            })
            .await;
    }
}
