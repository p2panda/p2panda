// SPDX-License-Identifier: MIT OR Apache-2.0

use std::cell::RefCell;
use std::collections::VecDeque;
use std::future::Future;
use std::marker::PhantomData;
use std::pin::{self, Pin};
use std::task::{Context, Poll, Waker};

use futures_util::{ready, FutureExt};
use p2panda_core::{Body, Extensions, Header, Operation};
use p2panda_store::LogStore;
use pin_utils::pin_mut;
use thiserror::Error;

use crate::ooo::validation::{validate_operation_retry, ValidationError, ValidationResult};

/// Optimized validation logic for incoming operations with some tolerance for "out of order"
/// behavior.
///
/// p2panda's append-only logs are totally ordered per author, with the exception of potential
/// forks. Due to race conditions or buggy implementations it is not always possible to assume that
/// operations really arrive in the order which is required, this is why we accept a certain amount
/// of unordered operations before we bail.
///
/// The internal buffer size determines the maximum number of out-of-order operations in a row this
/// method can handle. This means that given a buffer size of for example 100, we can handle a
/// worst-case unordered, fully reversed log with 100 items without problem.
#[derive(Debug)]
pub struct ValidationBuffer<S, L, E>
where
    S: LogStore<L, E> + Clone,
    E: Extensions,
{
    store: S,
    ooo_buffer_size: usize,
    ooo_buffer_queue: RefCell<VecDeque<ValidationAttempt<L, E>>>,
    waker: RefCell<Option<Waker>>,
    _marker: PhantomData<L>,
}

impl<S, L, E> ValidationBuffer<S, L, E>
where
    S: LogStore<L, E> + Clone,
    E: Extensions,
{
    pub fn new(store: S, ooo_buffer_size: usize) -> Self {
        Self {
            store,
            ooo_buffer_size,
            // @TODO(adz): We can optimize for the internal out-of-order buffer even more as it's
            // FIFO nature is not optimal. A sorted list (by seq num, maybe even grouped by public
            // key) might be more efficient, though I'm not sure about optimal implementations yet,
            // so benchmarks and more real-world experience might make sense before we attempt any
            // of this.
            ooo_buffer_queue: RefCell::new(VecDeque::with_capacity(ooo_buffer_size)),
            waker: RefCell::new(None),
            _marker: PhantomData,
        }
    }

    pub fn queue(
        &self,
        header: Header<E>,
        body: Option<Body>,
        header_bytes: Vec<u8>,
        log_id: L,
        prune_flag: bool,
    ) -> Result<(), ValidationBufferError> {
        if self.is_full() {
            return Err(ValidationBufferError::FullBuffer);
        }

        let attempt = ValidationAttempt {
            header,
            body,
            header_bytes,
            log_id,
            prune_flag,
            counter: 1,
        };

        // Push to the front of the queue so newly incoming operations are prioritized.
        {
            let mut queue = self.ooo_buffer_queue.borrow_mut();
            queue.push_front(attempt);
        }

        Ok(())
    }

    pub async fn next(&self) -> Result<Option<Operation<E>>, ValidationBufferError> {
        let attempt = {
            let mut queue = self.ooo_buffer_queue.borrow_mut();
            queue.pop_front()
        };

        let Some(attempt) = attempt else {
            return Ok(None);
        };

        self.validate(attempt).await
    }

    pub fn len(&self) -> usize {
        self.ooo_buffer_queue.borrow().len()
    }

    pub fn is_full(&self) -> bool {
        self.len() > self.ooo_buffer_size
    }

    async fn validate(
        &self,
        mut attempt: ValidationAttempt<L, E>,
    ) -> Result<Option<Operation<E>>, ValidationBufferError> {
        let result = {
            let mut store = self.store.clone();
            validate_operation_retry(
                &mut store,
                &attempt.header,
                attempt.body.as_ref(),
                &attempt.header_bytes,
                &attempt.log_id,
                attempt.prune_flag,
            )
            .await
        };

        self.waker.take().map(Waker::wake);

        // If the operation arrived out-of-order we can push it back into the internal buffer and
        // try again later (attempted for a configured number of times), otherwise forward the
        // result of ingest to the consumer.
        match result {
            Ok(ValidationResult::Valid(operation)) => Ok(Some(operation)),
            Ok(ValidationResult::Retry(header, body, header_bytes, num_missing)) => {
                // The number of max. reattempts is equal the size of the buffer. As long as the
                // buffer is just a FIFO queue it doesn't make sense to optimize over different
                // parameters as in a worst-case distribution of items (exact reverse) this will be
                // the max. and min. required bound.
                if attempt.counter > self.ooo_buffer_size {
                    return Err(ValidationBufferError::MaxAttemptsReached(num_missing));
                }
                attempt.counter += 1;

                if self.is_full() {
                    return Err(ValidationBufferError::FullBuffer);
                }

                // Push to the back of the queue as out-of-order operations are deprioritized.
                {
                    let mut queue = self.ooo_buffer_queue.borrow_mut();
                    queue.push_back(attempt);
                }

                Ok(None)
            }
            Err(err) => Err(ValidationBufferError::Validation(err)),
        }
    }
}

impl<S, L, E> Future for ValidationBuffer<S, L, E>
where
    S: LogStore<L, E> + Clone,
    E: Extensions,
{
    type Output = Result<Option<Operation<E>>, ValidationBufferError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let fut = async { self.next().await };
        pin_mut!(fut);

        match fut.poll_unpin(cx) {
            Poll::Ready(result) => Poll::Ready(result),
            Poll::Pending => {
                let mut inner = self.waker.borrow_mut();
                if let Some(waker) = inner.as_mut() {
                    waker.clone_from(cx.waker());
                } else {
                    inner.replace(cx.waker().clone());
                }

                Poll::Pending
            }
        }
    }
}

#[derive(Debug)]
struct ValidationAttempt<L, E> {
    header: Header<E>,
    body: Option<Body>,
    header_bytes: Vec<u8>,
    log_id: L,
    prune_flag: bool,
    counter: usize,
}

#[derive(Clone, Debug, Error)]
pub enum ValidationBufferError {
    /// Errors which can occur due to invalid operations or critical storage failures.
    #[error(transparent)]
    Validation(#[from] ValidationError),

    /// Given number of attempts to validate have been exhausted.
    #[error("too many attempts to ingest out-of-order operation ({0} behind in log)")]
    MaxAttemptsReached(u64),

    #[error("out-of-order buffer is full")]
    FullBuffer,
}

#[cfg(test)]
mod tests {
    use p2panda_core::Operation;
    use p2panda_store::MemoryStore;

    use crate::test_utils::{generate_operations, Extensions, Log, StreamName};

    use super::{ValidationBuffer, ValidationBufferError};

    #[tokio::test]
    #[allow(unused_variables)]
    async fn max_attempts_reached() {
        let store = MemoryStore::<StreamName, Extensions>::new();
        let buffer = ValidationBuffer::new(store, 2);

        let operations = generate_operations(4);

        let (header, body) = operations.get(3).unwrap();
        let hash = header.hash();
        let log_id: StreamName = header.extension().unwrap();
        let header_bytes = header.to_bytes();

        let result = buffer.queue(header.clone(), body.clone(), header_bytes, log_id, false);
        assert!(result.is_ok());
        assert_eq!(buffer.len(), 1, "buffer should contain one operation");

        let result = buffer.next().await;
        assert!(
            matches!(result, Ok(None)),
            "should not return anything on attempt 1"
        );
        assert_eq!(buffer.len(), 1, "buffer should contain one operation");

        let result = buffer.next().await;
        assert!(
            matches!(result, Ok(None)),
            "should not return anything on attempt 2"
        );
        assert_eq!(buffer.len(), 1, "buffer should contain one operation");

        let result = buffer.next().await;
        assert!(
            matches!(result, Err(ValidationBufferError::MaxAttemptsReached(3))),
            "should return error on attempt 3 with correct seq num"
        );
        assert_eq!(buffer.len(), 0, "buffer should be empty");
    }
}
