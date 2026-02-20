// SPDX-License-Identifier: MIT OR Apache-2.0

use std::cell::RefCell;
use std::convert::Infallible;
use std::marker::PhantomData;
use std::sync::atomic::AtomicU64;
use std::task::Poll;
use std::time::Duration;

use futures_util::{StreamExt, stream};
use tokio::{task, time};
use tokio_stream::wrappers::UnboundedReceiverStream;

use crate::processors::buffered::Buffer;
use crate::test_utils::{AsyncBuffer, assert_poll_eq};

use super::*;

/// Processor turning all strings into UPPERCASE.
#[derive(Default)]
struct UppercaseProcessor {
    outputs: RefCell<AsyncBuffer<String>>,
}

impl Processor<String> for UppercaseProcessor {
    type Output = String;

    type Error = Infallible;

    async fn process(&self, input: String) -> Result<(), Self::Error> {
        self.outputs.borrow_mut().push(input.to_uppercase());
        Ok(())
    }

    async fn next(&self) -> Result<Self::Output, Self::Error> {
        Ok(self.outputs.borrow_mut().pop().await)
    }
}

/// Processor adding a counter to any item.
struct CounterProcessor<T> {
    outputs: RefCell<AsyncBuffer<WithCounter<T>>>,
    counter: AtomicU64,
}

impl<T> CounterProcessor<T> {
    pub fn new() -> Self {
        Self {
            outputs: RefCell::new(AsyncBuffer::new()),
            counter: AtomicU64::new(0),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct WithCounter<T> {
    item: T,
    counter: u64,
}

impl<T> ToString for WithCounter<T>
where
    T: ToString,
{
    fn to_string(&self) -> String {
        format!("{}_{}", self.item.to_string(), self.counter)
    }
}

impl<T> Processor<T> for CounterProcessor<T> {
    type Output = WithCounter<T>;

    type Error = String;

    async fn process(&self, item: T) -> Result<(), Self::Error> {
        let counter = self
            .counter
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        self.outputs
            .borrow_mut()
            .push(WithCounter { item, counter });

        Ok(())
    }

    async fn next(&self) -> Result<Self::Output, Self::Error> {
        Ok(self.outputs.borrow_mut().pop().await)
    }
}

/// Test layer simulating "expensive" async operations when calling "next" or "process".
struct SlowProcessor<T> {
    process_delay: Duration,
    next_delay: Duration,
    output_queue: RefCell<AsyncBuffer<String>>,
    should_error: bool,
    _marker: PhantomData<T>,
}

impl<T> SlowProcessor<T>
where
    T: ToString,
{
    fn new() -> Self {
        Self {
            process_delay: Duration::from_millis(0),
            next_delay: Duration::from_millis(0),
            output_queue: RefCell::new(AsyncBuffer::new()),
            should_error: false,
            _marker: PhantomData,
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

impl<T> Processor<T> for SlowProcessor<T>
where
    T: ToString,
{
    type Output = String;

    type Error = String;

    async fn process(&self, input: T) -> Result<(), Self::Error> {
        time::sleep(self.process_delay).await;

        if self.should_error {
            return Err(format!("error in process method: {}", input.to_string()));
        }

        self.output_queue
            .borrow_mut()
            .push(format!("processed_{}", input.to_string()));

        Ok(())
    }

    async fn next(&self) -> Result<Self::Output, Self::Error> {
        time::sleep(self.next_delay).await;

        if self.should_error {
            return Err("error in next method".to_string());
        }

        Ok(self.output_queue.borrow_mut().pop().await)
    }
}

#[tokio::test]
async fn awaiting_on_next() {
    // Processors do not terminate and will rather await at "next" whenever there's no work.
    let uppercase = UppercaseProcessor::default();
    uppercase.process("Hello".to_string()).await.unwrap();
    assert_eq!(uppercase.next().await, Ok("HELLO".to_string()));
    assert_poll_eq(uppercase.next(), Poll::Pending);

    // Continue doing new work ..
    uppercase.process("World".to_string()).await.unwrap();
    assert_eq!(uppercase.next().await, Ok("WORLD".to_string()));
    assert_poll_eq(uppercase.next(), Poll::Pending);
}

#[tokio::test]
async fn chaining_processors() {
    let uppercase = UppercaseProcessor::default();
    let counter = CounterProcessor::<String>::new();

    let pipeline = PipelineBuilder::new()
        .layer(uppercase)
        .layer(counter)
        .build();

    pipeline.process("im".to_string()).await.unwrap();
    pipeline.process("very".to_string()).await.unwrap();
    pipeline.process("silent".to_string()).await.unwrap();

    assert_eq!(
        pipeline.next().await,
        Ok(WithCounter {
            item: "IM".to_string(),
            counter: 0,
        }),
    );

    assert_eq!(
        pipeline.next().await,
        Ok(WithCounter {
            item: "VERY".to_string(),
            counter: 1,
        }),
    );

    assert_eq!(
        pipeline.next().await,
        Ok(WithCounter {
            item: "SILENT".to_string(),
            counter: 2,
        }),
    );
}

#[tokio::test]
async fn expensive_async_processing() {
    let local = task::LocalSet::new();

    local
        .run_until(async move {
            // Have a "slow" processing layer which will not return a result instantly (first
            // poll will _not_ yield Poll::Ready(T)).
            let slow = SlowProcessor::new()
                .with_process_delay(Duration::from_millis(25))
                .with_next_delay(Duration::from_millis(50));
            assert_poll_eq(slow.process(0), Poll::Pending);
            assert_poll_eq(slow.next(), Poll::Pending);

            // .. eventually the result will arrive.
            slow.process(1).await.unwrap();
            let result = slow.next().await;
            assert_eq!(result, Ok("processed_1".to_string()));
        })
        .await;
}

#[tokio::test]
async fn expensive_async_processors_chaining() {
    let local = task::LocalSet::new();

    local
        .run_until(async move {
            let handle_1 = task::spawn_local(async {
                let slow_1 = SlowProcessor::new().with_next_delay(Duration::from_millis(10));
                let uppercase = UppercaseProcessor::default();
                let slow_2 = SlowProcessor::new().with_next_delay(Duration::from_millis(10));

                let pipeline = PipelineBuilder::new()
                    .layer(slow_1)
                    .layer(uppercase)
                    .layer(slow_2)
                    .build();

                pipeline.process(1).await.unwrap();
                pipeline.process(2).await.unwrap();
                pipeline.process(3).await.unwrap();

                assert_eq!(pipeline.next().await, Ok("processed_PROCESSED_1".into()));
                pipeline.process(4).await.unwrap();
                assert_eq!(pipeline.next().await, Ok("processed_PROCESSED_2".into()));
                assert_eq!(pipeline.next().await, Ok("processed_PROCESSED_3".into()));
                assert_eq!(pipeline.next().await, Ok("processed_PROCESSED_4".into()));
            });

            let handle_2 = task::spawn_local(async {
                let slow_1 = SlowProcessor::new()
                    .with_process_delay(Duration::from_millis(25))
                    .with_next_delay(Duration::from_millis(15));
                let uppercase = UppercaseProcessor::default();
                let slow_2 = SlowProcessor::new().with_next_delay(Duration::from_millis(10));

                let pipeline = PipelineBuilder::new()
                    .layer(slow_1)
                    .layer(uppercase)
                    .layer(slow_2)
                    .build();

                pipeline.process(1).await.unwrap();
                assert_eq!(pipeline.next().await, Ok("processed_PROCESSED_1".into()));
                pipeline.process(2).await.unwrap();
                pipeline.process(3).await.unwrap();
                assert_eq!(pipeline.next().await, Ok("processed_PROCESSED_2".into()));
                assert_eq!(pipeline.next().await, Ok("processed_PROCESSED_3".into()));
            });

            let (result_1, result_2) = futures_util::future::join(handle_1, handle_2).await;
            assert!(result_1.is_ok());
            assert!(result_2.is_ok());
        })
        .await;
}

#[tokio::test]
async fn error_handling() {
    let local = task::LocalSet::new();

    local
        .run_until(async move {
            let slow = SlowProcessor::new().with_error_mode();
            assert!(slow.process(1).await.is_err());
            assert!(slow.next().await.is_err());
        })
        .await;
}

#[tokio::test]
async fn buffered_processor() {
    let local = task::LocalSet::new();

    local
        .run_until(async move {
            let uppercase = UppercaseProcessor::default();
            let (_buffered, tx, rx) = Buffer::new(uppercase);

            tx.send("i scream".to_string()).unwrap();
            tx.send("you scream".to_string()).unwrap();
            tx.send("we all scream".to_string()).unwrap();
            tx.send("for icecream".to_string()).unwrap();

            let rx = UnboundedReceiverStream::new(rx);

            let result = rx
                .take(4) // Processor does not terminate by itself
                .filter_map(|item| async {
                    match item {
                        Ok(item) => Some(item),
                        Err(err) => panic!("{}", err),
                    }
                })
                .collect::<Vec<String>>()
                .await;

            assert_eq!(
                result,
                vec![
                    "I SCREAM".to_string(),
                    "YOU SCREAM".to_string(),
                    "WE ALL SCREAM".to_string(),
                    "FOR ICECREAM".to_string(),
                ]
            );
        })
        .await;
}

#[tokio::test]
async fn into_stream() {
    let local = task::LocalSet::new();

    local
        .run_until(async move {
            let slow = SlowProcessor::new()
                .with_process_delay(Duration::from_millis(25))
                .with_next_delay(Duration::from_millis(15));
            let uppercase = UppercaseProcessor::default();
            let counter = CounterProcessor::new();

            let pipeline = stream::iter([62, 23, 11, 74])
                .layer(counter)
                .filter_map(|item| async { item.ok() })
                .layer(slow)
                .filter_map(|item| async { item.ok() })
                .layer(uppercase)
                .filter_map(|item| async { item.ok() });

            assert_eq!(
                pipeline.take(4).collect::<Vec<String>>().await,
                vec![
                    "PROCESSED_62_0".to_string(),
                    "PROCESSED_23_1".to_string(),
                    "PROCESSED_11_2".to_string(),
                    "PROCESSED_74_3".to_string(),
                ],
            );
        })
        .await;
}

#[tokio::test]
async fn processor_stream_polling() {
    let local = task::LocalSet::new();

    local
        .run_until(async move {
            // Input stream and processor output stream are properly polled independently, so new
            // input doesn't get blocked when processor is still working.

            let input_stream = stream::iter(vec![
                "hey".to_string(),
                "ho".to_string(),
                "lets go".to_string(),
            ]);

            // Create a processor with delay on processing (_not_ on next).
            let slow_processor = SlowProcessor::new().with_process_delay(Duration::from_millis(50));
            let stream = slow_processor.into_stream(input_stream);

            // Collect all results - this should work without getting stuck even though the
            // processor has delays.
            let results: Vec<_> = stream
                .take(3)
                .filter_map(|item| async { item.ok() })
                .collect()
                .await;

            assert_eq!(results.len(), 3);
            assert!(results.iter().all(|s| s.starts_with("processed_")));
        })
        .await;
}
