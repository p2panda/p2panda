// SPDX-License-Identifier: MIT OR Apache-2.0

use std::cell::RefCell;
use std::collections::VecDeque;
use std::convert::Infallible;
use std::pin::Pin;

use tokio::sync::Notify;

use crate::Processor;

/// Hook to trigger custom actions by observing processor events passing through the pipeline.
pub trait ProcessorHook<T>: Send {
    fn on_input<'a>(&'a self, input: &'a T) -> impl Future<Output = ()> + Send + 'a;
}

type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

trait DynProcessorHook<T>: Send {
    fn on_input<'a>(&'a self, input: &'a T) -> BoxFuture<'a, ()>;
}

impl<I, T: ProcessorHook<I>> DynProcessorHook<I> for T {
    fn on_input<'a>(&'a self, input: &'a I) -> BoxFuture<'a, ()> {
        Box::pin(ProcessorHook::on_input(self, input))
    }
}

/// List of custom hooks which can be registered on a single processor layer.
#[derive(Default)]
pub struct ProcessorHooksList<T> {
    inner: Vec<Box<dyn DynProcessorHook<T> + Sync>>,
}

impl<T> ProcessorHooksList<T> {
    pub fn new() -> Self {
        Self { inner: Vec::new() }
    }

    pub fn push(&mut self, hook: impl ProcessorHook<T> + Sync + 'static) {
        self.inner.push(Box::new(hook));
    }
}

impl<T> ProcessorHook<T> for ProcessorHooksList<T>
where
    T: Sync,
{
    async fn on_input(&self, input: &T) {
        for hook in &self.inner {
            hook.on_input(input).await;
        }
    }
}

#[derive(Default)]
pub struct Hooks<T> {
    list: ProcessorHooksList<T>,
    notify: Notify,
    queue: RefCell<VecDeque<T>>,
}

impl<T> Hooks<T> {
    pub fn new() -> Self {
        Self::from_list(ProcessorHooksList::new())
    }

    pub fn from_list(list: ProcessorHooksList<T>) -> Self {
        Self {
            list,
            notify: Notify::new(),
            queue: RefCell::new(VecDeque::new()),
        }
    }

    pub fn push(&mut self, hook: impl ProcessorHook<T> + Sync + 'static) {
        self.list.push(hook);
    }
}

impl<T> Processor<T> for Hooks<T>
where
    T: Sync,
{
    type Output = T;

    type Error = Infallible;

    async fn process(&self, input: T) -> Result<(), Self::Error> {
        ProcessorHook::on_input(&self.list, &input).await;

        self.queue.borrow_mut().push_back(input);
        self.notify.notify_one();

        Ok(())
    }

    async fn next(&self) -> Result<Self::Output, Self::Error> {
        loop {
            if let Some(output) = self.queue.borrow_mut().pop_front() {
                return Ok(output);
            }

            self.notify.notified().await;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    use tokio::task;

    use crate::Processor;

    use super::{Hooks, ProcessorHook};

    #[tokio::test]
    async fn react_to_processor_events() {
        type Event = String;

        #[derive(Clone, Debug)]
        struct BooHook {
            result: Arc<AtomicBool>,
        }

        impl ProcessorHook<Event> for BooHook {
            async fn on_input(&self, input: &Event) {
                // Are you screaming already? .. if not, trigger hook!
                self.result
                    .swap(&input.to_uppercase() != input, Ordering::Relaxed);
            }
        }

        #[derive(Clone, Debug)]
        struct ZzzHook {
            result: Arc<AtomicBool>,
        }

        impl ProcessorHook<Event> for ZzzHook {
            async fn on_input(&self, input: &Event) {
                // Are you sleeping already? .. if not, trigger hook!
                self.result.swap(!input.is_empty(), Ordering::Relaxed);
            }
        }

        let local = task::LocalSet::new();

        local
            .run_until(async {
                let boo = BooHook {
                    result: Arc::default(),
                };

                let zzz = ZzzHook {
                    result: Arc::default(),
                };

                let mut processor = Hooks::<Event>::new();
                processor.push(boo.clone());
                processor.push(zzz.clone());

                processor.process("I'm not scared.".into()).await.unwrap();
                let _ = processor.next().await.unwrap();

                assert!(boo.result.load(Ordering::Relaxed), "BOO, WE SCARED YOU!");
                assert!(
                    zzz.result.load(Ordering::Relaxed),
                    "You should be sleeping!"
                );

                processor.process("AAAAAAAAAAH".into()).await.unwrap();
                let _ = processor.next().await.unwrap();

                assert!(!boo.result.load(Ordering::Relaxed), "AHAHAHAHA");
                assert!(zzz.result.load(Ordering::Relaxed), "Try to sleep!");

                processor.process("".into()).await.unwrap();
                let _ = processor.next().await.unwrap();

                assert!(!boo.result.load(Ordering::Relaxed), "Okay, you are silent.");
                assert!(
                    !zzz.result.load(Ordering::Relaxed),
                    "Good, you are sleeping. Good night!"
                );
            })
            .await;
    }
}
