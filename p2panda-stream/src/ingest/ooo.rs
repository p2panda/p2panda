// SPDX-License-Identifier: MIT OR Apache-2.0

use std::hash::Hash as StdHash;
use std::sync::Arc;

use indexmap::IndexMap;
use p2panda_core::{AnyHeader, Extensions, Hash, LogId, Operation};
use tokio::sync::Mutex;

#[derive(Debug)]
pub enum OooResult<'a, E> {
    /// Operation is already in-order and doesn't need buffering.
    InOrder(&'a Operation<E>),

    /// Operation freed buffered items which are now in-order.
    ///
    /// The incoming operation itself is also included in the array.
    Ordered(Vec<Operation<E>>),

    /// Operation is out-of-order and will be buffered.
    OutOfOrder,

    /// Operation is from before a pruning point and thus outdated.
    Outdated,
}

/// Out-of-order (ooo) buffer allowing a configurable window for handling operations with no
/// predecessors yet.
///
/// For every operation this buffer checks if it arrived out-of-order. If yes, it is stored in the
/// internal ring-buffer. If the buffer runs full the oldest item gets evicted first.
///
/// ## Example
///
/// If a log is at log height `[1]` (frontier) and an operation of sequence number `[3]` arrives, it
/// can not be appended to the log due to the strict nature of an append-only log. It will be pushed
/// into the buffer, awaiting the missing `[2]` operation:
///
/// ```text
///          [0] <- [1] <- Log in database
///
/// [3] <- Incoming operation
///
/// => Push into ooo-Buffer.
/// ```
///
/// Operation `[2]` arrives which will "free" the out-of-order items in the buffer, making them "in
/// order". It will release them from the buffer and forward to the user for further validation and
/// finally insertion into the database:
///
/// ```text
///          [0] <- [1] <- Log in database
///
///          [3] <- Operation in ooo-Buffer
///
/// [2] <- Incoming operation
///
/// => Return [2, 3]
/// ```
///
/// ## Assumptions
///
/// Please make sure to only process items which have been checked before against:
///
/// 1. Incoming operations from tombstoned logs / topics were filtered out before.
/// 2. Duplicate, already ingested operations have been filtered out before.
#[derive(Clone, Debug)]
pub struct OooBuffer<L, E>
where
    L: LogId,
    E: Extensions,
{
    buffer: Arc<Mutex<ChainRing<Hash, L, Operation<E>>>>,
}

impl<L, E> Default for OooBuffer<L, E>
where
    L: LogId,
    E: Extensions,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<L, E> OooBuffer<L, E>
where
    L: LogId,
    E: Extensions,
{
    pub fn new() -> Self {
        Self::with_capacity(128)
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            buffer: Arc::new(Mutex::new(ChainRing::with_capacity(capacity))),
        }
    }

    // TODO: Use AnyOperation when OperationStore is ready.
    pub async fn process<'a>(
        &self,
        operation: &'a Operation<E>,
        latest_header: Option<&AnyHeader>,
        log_id: &L,
        prune_flag: bool,
    ) -> OooResult<'a, E> {
        // Operation marks a prune point and all operations before that point become redundant,
        // including the ones which would theoretically be "freed" by it.
        //
        // ```text
        //          [0] <- Log in database
        //
        //          [2] <- Operation in ooo-Buffer
        //
        // [4] <- Incoming ooo-Operation
        //  ^
        //  prune_flag=true
        //
        // => Return [4]
        // ```
        //
        // Note that processing [4] will delete [0] in the database. [2] will remain in the
        // ooo-Buffer unused, until it gets evicted.
        if prune_flag {
            return OooResult::InOrder(operation);
        }

        match latest_header {
            Some(latest_header) => {
                if operation.header.seq_num < latest_header.seq_num {
                    // Operation is from _before_ the log frontier and thus redundant.
                    //
                    // Since we assumed that checks for duplicates already taken place, this case
                    // can only occur if the log was pruned and this operation belongs to the
                    // removed log-prefix.
                    //
                    // ```text
                    //          [7] <- [8] <- [9] <- Log in database
                    //           ^
                    //         pruned
                    //
                    // [4] <- Incoming ooo-Operation
                    //
                    // => Return nothing
                    // ```
                    OooResult::Outdated
                } else if latest_header.seq_num == operation.header.seq_num.saturating_sub_signed(1)
                {
                    // Operation is regular, next expected item in log, return it directly.
                    //
                    // ```text
                    // [0] <- [1] <- Log in database
                    //         ^
                    //      Frontier
                    //
                    // [2] <- Incoming Operation
                    //
                    // => Return [2]
                    // ```
                    OooResult::InOrder(operation)
                } else {
                    // Operation is _after_ the log frontier and thus out-of-order / can't be
                    // appended to log yet.
                    //
                    // ```text
                    //          [0] <- [1] <- Log in database
                    //
                    // [3] <- Incoming ooo-Operation
                    //
                    // => Push into ooo-Buffer.
                    // ```
                    //
                    // We then check if this item freed any operations in ring-buffer.
                    self.push_and_pop_from(operation, latest_header.backlink, log_id)
                        .await
                }
            }
            None => {
                // There's no log yet. We keep items in the buffer until it runs full or we're
                // building a valid log in memory.
                //
                // ```text
                //          [ ] <- Log in database (empty)
                //
                //          [1] <- Operation in ooo-Buffer
                //
                // [0] <- Incoming ooo-Operation
                //
                // => Return [0, 1]
                // ```
                //
                // Push and then check if this item freed any operations in ring-buffer.
                //
                // We set the expected backlink to `None`, indicating that we are looking for the
                // whole log / from seq_num=0.
                self.push_and_pop_from(operation, None, log_id).await
            }
        }
    }

    async fn push_and_pop_from<'a>(
        &self,
        operation: &'a Operation<E>,
        expected_backlink: Option<Hash>,
        log_id: &L,
    ) -> OooResult<'a, E> {
        let mut buffer = self.buffer.lock().await;

        // Push item to ring-buffer, this will eventually evict old items when full.
        buffer.push(
            operation.hash,
            operation.header.backlink,
            log_id.clone(),
            operation.clone(),
        );

        // We should check if this item freed any operations in ring-buffer / made them "in-order".
        // The check takes place from the current log frontier (`expected_backlink`) in the
        // database. If it's `None` we don't have any items for the log yet in the database.
        //
        // ```text
        //          [0] <- [1] <- Log in database
        //
        //          [3] <- Operation in ooo-Buffer
        //
        // [2] <- Incoming operation
        //
        // => Return [2, 3]
        // ```
        let result = buffer.pop_from(expected_backlink, log_id.clone());
        if result.is_empty() {
            OooResult::OutOfOrder
        } else {
            OooResult::Ordered(result)
        }
    }
}

#[derive(Debug)]
struct ChainRing<ID, L, T>
where
    ID: Copy + Eq + StdHash,
    L: Clone + Eq + StdHash,
{
    buffer: IndexMap<ChainRingKey<ID, L>, ChainRingValue<ID, T>>,
    capacity: usize,
}

#[derive(Debug, Eq, PartialEq, StdHash)]
struct ChainRingKey<ID, L>
where
    ID: Copy + Eq + StdHash,
    L: Clone + Eq + StdHash,
{
    backlink: Option<ID>,
    log_id: L,
}

#[derive(Debug)]
struct ChainRingValue<ID, T> {
    id: ID,
    item: T,
}

impl<ID, L, T> ChainRing<ID, L, T>
where
    ID: Copy + Eq + StdHash,
    L: Clone + Eq + StdHash,
{
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            buffer: IndexMap::with_capacity(capacity),
            capacity,
        }
    }

    pub fn push(&mut self, id: ID, backlink: Option<ID>, log_id: L, item: T) {
        if self.buffer.len() >= self.capacity {
            self.buffer.pop();
        }

        self.buffer.insert(
            ChainRingKey { backlink, log_id },
            ChainRingValue { id, item },
        );
    }

    /// Pop all items which have a complete chain from given position.
    ///
    /// The position is the id of the item _before_ the to-be-popped range:
    ///
    /// ```text
    /// [3] <- [4] <- [5] <- [6]
    ///  ^
    /// pop_from(3) -> [4, 5, 6]
    /// ```
    pub fn pop_from(&mut self, backlink: Option<ID>, log_id: L) -> Vec<T> {
        let mut result = Vec::new();

        let mut next = ChainRingKey {
            backlink,
            log_id: log_id.clone(),
        };

        while let Some(ChainRingValue { id, item }) = self.buffer.shift_remove(&next) {
            result.push(item);
            next = ChainRingKey {
                backlink: Some(id),
                log_id: log_id.clone(),
            };
        }

        result
    }

    #[allow(unused)]
    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    #[allow(unused)]
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    #[allow(unused)]
    pub fn clear(&mut self) {
        self.buffer.clear()
    }
}

#[cfg(test)]
mod tests {
    use super::ChainRing;

    #[test]
    fn push_and_pop_from() {
        let mut ring = ChainRing::with_capacity(64);

        // Form a chain: 4 <- [5] <- [6] <- [7]
        ring.push(5, Some(4), "test-log", 5);
        ring.push(6, Some(5), "test-log", 6);
        ring.push(7, Some(6), "test-log", 7);
        assert_eq!(ring.len(), 3);

        // Try to pop chain range from 3 on, but item [4] is missing.
        assert!(ring.pop_from(Some(3), "test-log").is_empty());

        // Add item [4] to chain: 3 <- [4] <- [5] <- [6] <- [7]
        ring.push(4, Some(3), "test-log", 4);
        assert_eq!(ring.len(), 4);

        // Pop chain range from 3 on.
        assert_eq!(ring.pop_from(Some(3), "test-log"), vec![4, 5, 6, 7]);
        assert_eq!(ring.len(), 0);
    }

    #[test]
    fn log_from_beginning() {
        let mut ring = ChainRing::with_capacity(64);
        ring.push(0, None, "test-log", 0);
        ring.push(1, Some(0), "test-log", 1);
        ring.push(2, Some(1), "test-log", 2);
        assert_eq!(ring.len(), 3);
        assert_eq!(ring.pop_from(None, "test-log"), vec![0, 1, 2]);
    }

    #[test]
    fn ring_buffer() {
        let mut ring = ChainRing::with_capacity(2);
        ring.push(1, Some(0), "test-log", 1);
        ring.push(2, Some(1), "test-log", 2);
        ring.push(3, Some(2), "test-log", 3);
        assert_eq!(ring.len(), 2);
    }
}
