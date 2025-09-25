// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashSet;

/// Trait defining a store API for handling ready and pending dependencies backing causal / partial
/// ordering implementations.
///
/// An implementation of this store trait provides the following functionality:
///
/// - Maintain a list of all items which have all their dependencies met
/// - Maintain a list of items which don't have their dependencies met
/// - Return all pending items which depend on a given item
pub trait OrdererStore<T> {
    type Error;

    /// Add an item to the store which has all it's dependencies met already. If this is the first
    /// time the item has been added it should also be pushed to the end of a "ready" queue.
    fn mark_ready(&self, key: T) -> impl Future<Output = Result<bool, Self::Error>>;

    /// Add an item which does not have all it's dependencies met yet.
    fn mark_pending(
        &self,
        key: T,
        dependencies: Vec<T>,
    ) -> impl Future<Output = Result<bool, Self::Error>>;

    /// Get all pending items which directly depend on the given key.
    fn get_next_pending(
        &self,
        key: T,
    ) -> impl Future<Output = Result<Option<HashSet<(T, Vec<T>)>>, Self::Error>>;

    /// Take the next ready item from the ready queue.
    fn take_next_ready(&self) -> impl Future<Output = Result<Option<T>, Self::Error>>;

    /// Remove all items from the pending queue which depend on the passed key.
    fn remove_pending(&self, key: T) -> impl Future<Output = Result<bool, Self::Error>>;

    /// Returns `true` if all the passed keys are present in the ready list.
    fn ready(&self, keys: &[T]) -> impl Future<Output = Result<bool, Self::Error>>;
}
