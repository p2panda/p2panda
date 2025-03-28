// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt::Debug;
use std::hash::Hash as StdHash;

use crate::partial::PartialOrderError;

/// Trait defining a store API for handling ready and pending dependencies.
///
/// An implementation of this store trait provides the following functionality:
/// - maintain a list of all items which have all their dependencies met
/// - maintain a list of items which don't have their dependencies met
/// - return all pending items which depend on a given item
#[allow(async_fn_in_trait)]
pub trait PartialOrderStore<K>
where
    K: Clone + Copy + StdHash + PartialEq + Eq,
{
    /// Add an item to the store which has all it's dependencies met already. If this is the first
    /// time the item has been added it should also be pushed to the end of a "ready" queue.
    async fn mark_ready(&mut self, key: K) -> Result<bool, PartialOrderError>;

    /// Add an item which does not have all it's dependencies met yet.
    async fn mark_pending(
        &mut self,
        key: K,
        dependencies: Vec<K>,
    ) -> Result<bool, PartialOrderError>;

    /// Get all pending items which directly depend on the given key.
    async fn get_next_pending(
        &self,
        key: K,
    ) -> Result<Option<HashSet<(K, Vec<K>)>>, PartialOrderError>;

    /// Take the next ready item from the ready queue.
    async fn take_next_ready(&mut self) -> Result<Option<K>, PartialOrderError>;

    /// Remove all items from the pending queue which depend on the passed key.
    async fn remove_pending(&mut self, key: K) -> Result<bool, PartialOrderError>;

    /// Returns `true` of all the passed keys are present in the ready list.
    async fn ready(&self, keys: &[K]) -> Result<bool, PartialOrderError>;
}

/// Memory implementation of the `PartialOrderStore` trait.
#[derive(Clone)]
pub struct MemoryStore<K> {
    pub(crate) ready: HashSet<K>,
    pub(crate) ready_queue: VecDeque<K>,
    pub(crate) pending: HashMap<K, HashSet<(K, Vec<K>)>>,
}

impl<K> Default for MemoryStore<K> {
    fn default() -> Self {
        Self {
            ready: HashSet::new(),
            ready_queue: VecDeque::new(),
            pending: HashMap::new(),
        }
    }
}

impl<K> PartialOrderStore<K> for MemoryStore<K>
where
    K: Clone + Copy + Debug + StdHash + PartialEq + Eq,
{
    async fn mark_ready(&mut self, key: K) -> Result<bool, PartialOrderError> {
        let result = self.ready.insert(key);
        if result {
            self.ready_queue.push_back(key);
        }
        Ok(result)
    }

    async fn mark_pending(
        &mut self,
        key: K,
        dependencies: Vec<K>,
    ) -> Result<bool, PartialOrderError> {
        let insert_occured = false;
        for dep_key in &dependencies {
            if self.ready.contains(dep_key) {
                continue;
            }

            let dependents = self.pending.entry(*dep_key).or_default();
            dependents.insert((key, dependencies.clone()));
        }

        Ok(insert_occured)
    }

    async fn get_next_pending(
        &self,
        key: K,
    ) -> Result<Option<HashSet<(K, Vec<K>)>>, PartialOrderError> {
        Ok(self.pending.get(&key).cloned())
    }

    async fn take_next_ready(&mut self) -> Result<Option<K>, PartialOrderError> {
        Ok(self.ready_queue.pop_front())
    }

    async fn remove_pending(&mut self, key: K) -> Result<bool, PartialOrderError> {
        Ok(self.pending.remove(&key).is_some())
    }

    async fn ready(&self, dependencies: &[K]) -> Result<bool, PartialOrderError> {
        let deps_set = HashSet::from_iter(dependencies.iter().cloned());
        let result = self.ready.is_superset(&deps_set);
        Ok(result)
    }
}
