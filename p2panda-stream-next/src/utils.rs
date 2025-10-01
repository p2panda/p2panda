// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::VecDeque;

use tokio::sync::{Mutex, Notify};

/// Simple async queue which awaits when trying to pop from it while it is empty.
#[derive(Debug, Default)]
pub struct AsyncBuffer<T> {
    queue: Mutex<VecDeque<T>>,
    notify: Notify,
}

#[allow(unused)]
impl<T> AsyncBuffer<T> {
    pub fn new() -> Self {
        Self {
            queue: Mutex::new(VecDeque::new()),
            notify: Notify::new(),
        }
    }

    pub async fn push(&self, item: T) {
        self.queue.lock().await.push_back(item);
        self.notify.notify_one(); // Wake up any pending recv
    }

    pub async fn extend(&self, items: Vec<T>) {
        self.queue.lock().await.extend(items);
        self.notify.notify_one(); // Wake up any pending recv
    }

    pub async fn pop(&self) -> T {
        loop {
            if let Some(item) = self.queue.lock().await.pop_front() {
                return item;
            }

            // Wait for notification that an item was added.
            self.notify.notified().await;
        }
    }
}
