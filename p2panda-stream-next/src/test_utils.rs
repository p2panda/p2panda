// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::VecDeque;
use std::task::Poll;

use futures_test::task::noop_context;
use tokio::pin;
use tokio::sync::Notify;

/// Simple async queue which awaits when trying to pop from it while it is empty.
#[derive(Debug, Default)]
pub struct AsyncBuffer<T> {
    queue: VecDeque<T>,
    notify: Notify,
}

impl<T> AsyncBuffer<T> {
    pub fn new() -> Self {
        Self {
            queue: VecDeque::new(),
            notify: Notify::new(),
        }
    }

    pub fn push(&mut self, item: T) {
        self.queue.push_back(item);
        self.notify.notify_one(); // Wake up any pending recv
    }

    pub async fn pop(&mut self) -> T {
        loop {
            if let Some(item) = self.queue.pop_front() {
                return item;
            }

            // Wait for notification that an item was added.
            self.notify.notified().await;
        }
    }

    #[allow(dead_code)]
    pub fn try_pop(&mut self) -> Option<T> {
        self.queue.pop_front()
    }
}

/// Compare the resulting poll state from a future.
pub fn assert_poll_eq<Fut: Future>(fut: Fut, poll: Poll<Fut::Output>)
where
    <Fut as futures_core::Future>::Output: PartialEq + std::fmt::Debug,
{
    assert_eq!(
        {
            pin!(fut);
            let mut cx = noop_context();
            fut.poll(&mut cx)
        },
        poll,
    );
}
