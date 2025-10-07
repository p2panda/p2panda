// SPDX-License-Identifier: MIT OR Apache-2.0

use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::convert::Infallible;
use std::fmt::Debug;
use std::hash::Hash as StdHash;
use std::rc::Rc;

use crate::memory::MemoryStore;
use crate::orderer::OrdererStore;
#[cfg(any(test, feature = "test_utils"))]
use crate::orderer::OrdererTestExt;

#[allow(clippy::type_complexity)]
#[derive(Clone, Debug)]
pub struct OrdererMemoryStore<ID> {
    ready: Rc<RefCell<HashSet<ID>>>,
    ready_queue: Rc<RefCell<VecDeque<ID>>>,
    pending: Rc<RefCell<HashMap<ID, HashSet<(ID, Vec<ID>)>>>>,
}

impl<ID> OrdererMemoryStore<ID> {
    pub fn new() -> Self {
        Self {
            ready: Rc::new(RefCell::new(HashSet::new())),
            ready_queue: Rc::new(RefCell::new(VecDeque::new())),
            pending: Rc::new(RefCell::new(HashMap::new())),
        }
    }
}

impl<ID> Default for OrdererMemoryStore<ID> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T, ID> OrdererStore<ID> for MemoryStore<T, ID>
where
    T: Debug,
    ID: Clone + Eq + Debug + StdHash,
{
    type Error = Infallible;

    async fn mark_ready(&self, id: ID) -> Result<bool, Infallible> {
        let result = self.orderer.ready.borrow_mut().insert(id.clone());

        // Always push to queue, even if item was already processed once, we want the system to
        // have similar behaviour when re-processing the same items (with idempotency).
        self.orderer.ready_queue.borrow_mut().push_back(id);

        Ok(result)
    }

    async fn mark_pending(&self, id: ID, dependencies: Vec<ID>) -> Result<bool, Infallible> {
        let mut insert_occured = false;

        for dep_key in &dependencies {
            if self.orderer.ready.borrow().contains(dep_key) {
                continue;
            }

            let mut pending = self.orderer.pending.borrow_mut();
            let dependents = pending.entry(dep_key.clone()).or_default();
            if dependents.insert((id.clone(), dependencies.clone())) {
                insert_occured = true;
            }
        }

        Ok(insert_occured)
    }

    async fn get_next_pending(&self, id: ID) -> Result<Option<HashSet<(ID, Vec<ID>)>>, Infallible> {
        Ok(self.orderer.pending.borrow().get(&id).cloned())
    }

    async fn take_next_ready(&self) -> Result<Option<ID>, Infallible> {
        Ok(self.orderer.ready_queue.borrow_mut().pop_front())
    }

    async fn remove_pending(&self, id: ID) -> Result<bool, Infallible> {
        Ok(self.orderer.pending.borrow_mut().remove(&id).is_some())
    }

    async fn ready(&self, dependencies: &[ID]) -> Result<bool, Infallible> {
        let deps_set = HashSet::from_iter(dependencies.iter().cloned());
        let result = self.orderer.ready.borrow().is_superset(&deps_set);
        Ok(result)
    }
}

#[cfg(any(test, feature = "test_utils"))]
impl<T, ID> OrdererTestExt for MemoryStore<T, ID>
where
    T: Debug,
    ID: Debug,
{
    async fn ready_len(&self) -> usize {
        self.orderer.ready.borrow().len()
    }

    async fn ready_queue_len(&self) -> usize {
        self.orderer.ready_queue.borrow().len()
    }

    async fn pending_len(&self) -> usize {
        self.orderer.pending.borrow().len()
    }
}
