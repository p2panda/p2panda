// SPDX-License-Identifier: MIT OR Apache-2.0

use std::cell::RefCell;
use std::collections::VecDeque;
use std::pin::Pin;

use tokio::sync::Notify;

use crate::Processor;

/// Hook to trigger custom actions by observing processor events passing through the pipeline.
pub trait ProcessorHook<T>: std::fmt::Debug {
    fn on_input<'a>(&'a self, input: &'a T) -> impl Future<Output = ()> + 'a;
}

type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + 'a>>;

trait DynProcessorHook<T>: std::fmt::Debug {
    fn on_input<'a>(&'a self, input: &'a T) -> BoxFuture<'a, ()>;
}

impl<I, T: ProcessorHook<I> + 'static> DynProcessorHook<I> for T {
    fn on_input<'a>(&'a self, input: &'a I) -> BoxFuture<'a, ()> {
        Box::pin(ProcessorHook::on_input(self, input))
    }
}

/// List of custom hooks which can be registered on a single processor layer.
#[derive(Debug, Default)]
pub struct ProcessorHooksList<T> {
    inner: Vec<Box<dyn DynProcessorHook<T>>>,
}

impl<T> ProcessorHooksList<T> {
    pub fn new() -> Self {
        Self { inner: Vec::new() }
    }

    pub fn push(&mut self, hook: impl ProcessorHook<T> + 'static) {
        self.inner.push(Box::new(hook));
    }
}

impl<T> ProcessorHook<T> for ProcessorHooksList<T>
where
    T: std::fmt::Debug,
{
    async fn on_input(&self, input: &T) {
        for hook in &self.inner {
            hook.on_input(input).await;
        }
    }
}

#[derive(Debug, Default)]
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

    pub fn push(&mut self, hook: impl ProcessorHook<T> + 'static) {
        self.list.push(hook);
    }
}

impl<T> Processor<T> for Hooks<T>
where
    T: std::fmt::Debug,
{
    type Output = T;

    type Error = T;

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
    use std::cell::RefCell;
    use std::rc::Rc;

    use tokio::task;

    use crate::Processor;

    use super::{Hooks, ProcessorHook};

    #[tokio::test]
    async fn react_to_processor_events() {
        type Event = String;

        #[derive(Clone, Debug)]
        struct BooHook {
            result: Rc<RefCell<bool>>,
        }

        impl ProcessorHook<Event> for BooHook {
            async fn on_input(&self, input: &Event) {
                // Are you screaming already? .. if not, trigger hook!
                self.result.replace_with(|_| &input.to_uppercase() != input);
            }
        }

        #[derive(Clone, Debug)]
        struct ZzzHook {
            result: Rc<RefCell<bool>>,
        }

        impl ProcessorHook<Event> for ZzzHook {
            async fn on_input(&self, input: &Event) {
                // Are you sleeping already? .. if not, trigger hook!
                self.result.replace_with(|_| !input.is_empty());
            }
        }

        let local = task::LocalSet::new();

        local
            .run_until(async {
                let boo = BooHook {
                    result: Rc::default(),
                };

                let zzz = ZzzHook {
                    result: Rc::default(),
                };

                let mut processor = Hooks::<Event>::new();
                processor.push(boo.clone());
                processor.push(zzz.clone());

                processor.process("I'm not scared.".into()).await.unwrap();
                let _ = processor.next().await.unwrap();

                assert!(*boo.result.borrow(), "BOO, WE SCARED YOU!");
                assert!(*zzz.result.borrow(), "You should be sleeping!");

                processor.process("AAAAAAAAAAH".into()).await.unwrap();
                let _ = processor.next().await.unwrap();

                assert!(!*boo.result.borrow(), "AHAHAHAHA");
                assert!(*zzz.result.borrow(), "Try to sleep!");

                processor.process("".into()).await.unwrap();
                let _ = processor.next().await.unwrap();

                assert!(!*boo.result.borrow(), "Okay, you are silent.");
                assert!(!*zzz.result.borrow(), "Good, you are sleeping. Good night!");
            })
            .await;
    }
}
