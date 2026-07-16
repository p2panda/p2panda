// SPDX-License-Identifier: MIT OR Apache-2.0

use std::sync::Arc;
use std::sync::atomic::AtomicUsize;

use p2panda_core::Topic;
use p2panda_net::utils::ShortFormat;
use tokio_util::sync::CancellationToken;
use tracing::trace;

/// Helper maintaining a counter of references to the same stream topic.
///
/// This is used to break out of relevant stream processing loops once all publisher and
/// subscription handles have been dropped.
#[derive(Debug)]
pub struct StreamDropGuard {
    topic: Topic,
    counter: Arc<AtomicUsize>,
    token: CancellationToken,
    ignore_drop: bool,
}

/// Initial value the reference counter starts with.
const INITIAL_COUNTER: usize = 1;

impl StreamDropGuard {
    pub(crate) fn new(topic: Topic, token: CancellationToken) -> Self {
        trace!(
            topic = topic.fmt_short(),
            counter = INITIAL_COUNTER,
            "new stream drop guard"
        );

        Self {
            topic,
            counter: Arc::new(AtomicUsize::new(INITIAL_COUNTER)),
            token,
            ignore_drop: false,
        }
    }

    /// Returns current number of references to this topic.
    #[allow(unused)]
    fn counter(&self) -> usize {
        self.counter.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Returns true if there's still one or more references for this topic used.
    #[allow(unused)]
    fn has_subscriptions(&self) -> bool {
        self.counter() >= INITIAL_COUNTER
    }

    /// Clone guard, but don't increment reference counter.
    ///
    /// This is useful if we need to keep it around somewhere for further use without affecting the
    /// drop logic.
    #[allow(unused)]
    fn clone_without_increment(&self) -> Self {
        Self {
            topic: self.topic,
            counter: self.counter.clone(),
            token: self.token.clone(),
            ignore_drop: true,
        }
    }
}

impl Clone for StreamDropGuard {
    fn clone(&self) -> Self {
        let value = self
            .counter
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);

        trace!(
            topic = self.topic.fmt_short(),
            counter = value + 1,
            "clone stream drop guard +1"
        );

        Self {
            topic: self.topic,
            counter: self.counter.clone(),
            token: self.token.clone(),
            ignore_drop: false,
        }
    }
}

impl Drop for StreamDropGuard {
    fn drop(&mut self) {
        // This instance is not used to count references, we drop it without taking any action.
        if self.ignore_drop {
            return;
        }

        // Check if we can cancel the stream processing token if all publishers and subscriptions
        // have been dropped for it.
        let previous_counter = self
            .counter
            .fetch_sub(1, std::sync::atomic::Ordering::SeqCst);

        trace!(
            topic = self.topic.fmt_short(),
            counter = previous_counter - 1,
            "drop stream drop guard -1"
        );

        // If the previous value is equal the initial value, the last instance of the guard was
        // dropped and the counter has no references to the topic anymore.
        let no_references_left = previous_counter == INITIAL_COUNTER;

        if no_references_left {
            trace!(
                topic = self.topic.fmt_short(),
                "cancel processed_stream token"
            );

            self.token.cancel();
        }
    }
}

#[cfg(test)]
mod tests {
    use tokio_util::sync::CancellationToken;

    use super::StreamDropGuard;

    #[tokio::test]
    async fn stream_drop_guard() {
        let token = CancellationToken::new();

        let guard_1 = StreamDropGuard::new([1; 32].into(), token.clone());
        assert_eq!(guard_1.counter(), 1);

        let guard_2 = guard_1.clone();
        assert_eq!(guard_1.counter(), 2);

        let guard_3 = guard_1.clone();
        assert_eq!(guard_1.counter(), 3);

        drop(guard_3);
        assert_eq!(guard_1.counter(), 2);
        assert!(!token.is_cancelled());

        drop(guard_2);
        assert_eq!(guard_1.counter(), 1);
        assert!(!token.is_cancelled());

        drop(guard_1);
        assert!(token.is_cancelled());
    }
}
