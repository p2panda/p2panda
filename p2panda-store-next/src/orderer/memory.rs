// SPDX-License-Identifier: MIT OR Apache-2.0

use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::convert::Infallible;
use std::hash::Hash as StdHash;
use std::rc::Rc;

use crate::memory::MemoryStore;
use crate::orderer::OrdererStore;

/// In-Memory database implementation of the `OrdererStore` trait.
#[allow(clippy::type_complexity)]
#[derive(Clone)]
pub struct OrdererMemoryStore<K> {
    pub(crate) ready: Rc<RefCell<HashSet<K>>>,
    pub(crate) ready_queue: Rc<RefCell<VecDeque<K>>>,
    pub(crate) pending: Rc<RefCell<HashMap<K, HashSet<(K, Vec<K>)>>>>,
}

impl<K> OrdererMemoryStore<K> {
    pub fn new() -> Self {
        Self {
            ready: Rc::new(RefCell::new(HashSet::new())),
            ready_queue: Rc::new(RefCell::new(VecDeque::new())),
            pending: Rc::new(RefCell::new(HashMap::new())),
        }
    }
}

impl<K> Default for OrdererMemoryStore<K> {
    fn default() -> Self {
        Self::new()
    }
}

impl<K> OrdererStore<K> for MemoryStore<K>
where
    K: Copy + Eq + StdHash,
{
    type Error = Infallible;

    async fn mark_ready(&self, key: K) -> Result<bool, Infallible> {
        let result = self.orderer.ready.borrow_mut().insert(key);
        if result {
            self.orderer.ready_queue.borrow_mut().push_back(key);
        }
        Ok(result)
    }

    async fn mark_pending(&self, key: K, dependencies: Vec<K>) -> Result<bool, Infallible> {
        let insert_occured = false;
        for dep_key in &dependencies {
            if self.orderer.ready.borrow().contains(dep_key) {
                continue;
            }

            let mut pending = self.orderer.pending.borrow_mut();
            let dependents = pending.entry(*dep_key).or_default();
            dependents.insert((key, dependencies.clone()));
        }

        Ok(insert_occured)
    }

    async fn get_next_pending(&self, key: K) -> Result<Option<HashSet<(K, Vec<K>)>>, Infallible> {
        Ok(self.orderer.pending.borrow().get(&key).cloned())
    }

    async fn take_next_ready(&self) -> Result<Option<K>, Infallible> {
        Ok(self.orderer.ready_queue.borrow_mut().pop_front())
    }

    async fn remove_pending(&self, key: K) -> Result<bool, Infallible> {
        Ok(self.orderer.pending.borrow_mut().remove(&key).is_some())
    }

    async fn ready(&self, dependencies: &[K]) -> Result<bool, Infallible> {
        let deps_set = HashSet::from_iter(dependencies.iter().cloned());
        let result = self.orderer.ready.borrow().is_superset(&deps_set);
        Ok(result)
    }
}

// Test abstraction for other crates so they can write tests without getting caught up by
// implementation details of the storage layer in this crate.
#[cfg(any(test, feature = "test_utils"))]
pub trait OrdererTestExt {
    fn ready_len(&self) -> usize;

    fn ready_queue_len(&self) -> usize;

    fn pending_len(&self) -> usize;
}

#[cfg(any(test, feature = "test_utils"))]
impl<K> OrdererTestExt for MemoryStore<K> {
    fn ready_len(&self) -> usize {
        self.orderer.ready.borrow().len()
    }

    fn ready_queue_len(&self) -> usize {
        self.orderer.ready_queue.borrow().len()
    }

    fn pending_len(&self) -> usize {
        self.orderer.pending.borrow().len()
    }
}
