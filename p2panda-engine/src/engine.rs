// SPDX-License-Identifier: AGPL-3.0-or-later

use std::cell::RefCell;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context as TaskContext, Poll, Waker};

use futures_core::stream::Stream;
use futures_util::future::{poll_fn, FutureExt};
use p2panda_core::{Extension, Operation};

use crate::boxed::BoxedMiddlewareInner;
use crate::context::Context;
use crate::ingest::IngestResult;
use crate::queue::Queue;
use crate::router::Router;

const DEFAULT_BUFFER_SIZE: usize = 256;

pub struct EngineBuilder<E>
where
    E: Extension,
{
    router: Option<Router<E>>,
    buffer: Option<usize>,
}

impl<E> Default for EngineBuilder<E>
where
    E: Extension,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<E> EngineBuilder<E>
where
    E: Extension,
{
    pub fn new() -> Self {
        Self {
            router: None,
            buffer: None,
        }
    }

    pub fn buffer_size(mut self, buffer: usize) -> Self {
        self.buffer = Some(buffer);
        self
    }

    pub fn router(mut self, router: Router<E>) -> Self {
        self.router = Some(router);
        self
    }

    pub fn build(self) -> Engine<E> {
        let router = self.router.unwrap_or_default();
        let buffer = self.buffer.unwrap_or(DEFAULT_BUFFER_SIZE);

        Engine::new(router, buffer)
    }
}

#[derive(Clone)]
pub struct Engine<E>
where
    E: Extension,
{
    inner: Rc<EngineInner<E>>,
}

pub struct EngineInner<E>
where
    E: Extension,
{
    context: Context,
    router: RefCell<Router<E>>,
    queue: RefCell<Queue<E>>,
    rx_task: RefCell<Option<Waker>>,
    tx_task: RefCell<Option<Waker>>,
}

impl<E> Engine<E>
where
    E: Extension,
{
    fn new(router: Router<E>, buffer: usize) -> Self {
        Self {
            inner: Rc::new(EngineInner {
                context: Context::new(),
                router: RefCell::new(router),
                queue: RefCell::new(Queue::new(buffer)),
                rx_task: RefCell::default(),
                tx_task: RefCell::default(),
            }),
        }
    }

    pub async fn ingest(&mut self, operation: Operation<E>) {
        poll_fn(|cx: &mut TaskContext<'_>| {
            // Silently ignore duplicates
            if self.inner.queue.borrow().contains(&operation) {
                return Poll::Ready(());
            }

            // @TODO: Check store for duplicates
            // @TODO: Basic log validation

            if self.inner.queue.borrow().is_full() {
                {
                    let mut tx_task = self.inner.tx_task.borrow_mut();
                    if let Some(waker) = tx_task.as_mut() {
                        waker.clone_from(cx.waker());
                    } else {
                        tx_task.replace(cx.waker().clone());
                    }
                }

                Poll::Pending
            } else {
                {
                    let mut queue = self.inner.queue.borrow_mut();
                    queue.push(operation.clone());
                }

                {
                    let rx_task = self.inner.rx_task.take();
                    if let Some(_rx_task) = rx_task {
                        Waker::wake(_rx_task)
                    }
                }

                Poll::Ready(())
            }
        })
        .await
    }
}

impl<E> Stream for Engine<E>
where
    E: Extension + 'static,
{
    type Item = IngestResult<E>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut TaskContext<'_>) -> Poll<Option<Self::Item>> {
        let queue = self.inner.queue.borrow();
        let operation = queue.front();

        match operation {
            Some(operation) => {
                let mut router = self.inner.router.borrow_mut();
                let result = match router
                    .ingest(self.inner.context.clone(), operation.to_owned())
                    .poll_unpin(cx)
                {
                    Poll::Ready(result) => result,
                    Poll::Pending => return Poll::Pending,
                };

                drop(queue);

                {
                    let mut queue = self.inner.queue.borrow_mut();
                    queue.pop();
                }

                {
                    let tx_task = self.inner.tx_task.take();
                    if let Some(_tx_task) = tx_task {
                        Waker::wake(_tx_task)
                    }
                }

                Poll::Ready(Some(result))
            }
            None => {
                {
                    let mut rx_task = self.inner.rx_task.borrow_mut();
                    if let Some(waker) = rx_task.as_mut() {
                        waker.clone_from(cx.waker());
                    } else {
                        rx_task.replace(cx.waker().clone());
                    }
                }

                Poll::Pending
            }
        }
    }
}
