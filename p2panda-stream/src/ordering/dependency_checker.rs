// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt::{Debug, Display};
use std::hash::Hash as StdHash;

use thiserror::Error;

pub trait DependencyStore<K>
where
    K: Clone + Copy + StdHash + PartialEq + Eq,
{
    async fn add_ready(&mut self, key: K) -> Result<bool, DependencyCheckerError>;

    async fn add_pending(
        &mut self,
        key: K,
        dependencies: Vec<K>,
    ) -> Result<bool, DependencyCheckerError>;

    async fn get_next_pending(
        &self,
        key: K,
    ) -> Result<Option<HashSet<(K, Vec<K>)>>, DependencyCheckerError>;

    async fn remove_pending(&mut self, key: K) -> Result<bool, DependencyCheckerError>;

    async fn ready(&self, keys: &[K]) -> Result<bool, DependencyCheckerError>;
}

#[derive(Clone, Default)]
pub struct MemoryStore<K> {
    ready: HashSet<K>,
    pending: HashMap<K, HashSet<(K, Vec<K>)>>,
}

impl<K> DependencyStore<K> for MemoryStore<K>
where
    K: Clone + Copy + Debug + StdHash + PartialEq + Eq,
{
    async fn add_ready(&mut self, key: K) -> Result<bool, DependencyCheckerError> {
        let result = self.ready.insert(key);
        Ok(result)
    }

    async fn add_pending(
        &mut self,
        key: K,
        dependencies: Vec<K>,
    ) -> Result<bool, DependencyCheckerError> {
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
    ) -> Result<Option<HashSet<(K, Vec<K>)>>, DependencyCheckerError> {
        Ok(self.pending.get(&key).cloned())
    }

    async fn remove_pending(&mut self, key: K) -> Result<bool, DependencyCheckerError> {
        Ok(self.pending.remove(&key).is_some())
    }

    async fn ready(&self, dependencies: &[K]) -> Result<bool, DependencyCheckerError> {
        let deps_set = HashSet::from_iter(dependencies.iter().cloned());
        let result = self.ready.is_superset(&deps_set);
        Ok(result)
    }
}

#[derive(Debug, Error)]
pub enum DependencyCheckerError {
    #[error("store error: {0}")]
    StoreError(String),
}

// @TODO: eventually processed and pending_queue needs to be populated from the database.
#[derive(Debug)]
pub struct DependencyChecker<K, S> {
    store: S,

    /// Queue of items whose dependencies are met. These are returned from calls to `next()`.
    ready_queue: VecDeque<K>,
}

impl<K, S> DependencyChecker<K, S>
where
    K: Clone + Copy + Display + StdHash + PartialEq + Eq,
    S: DependencyStore<K>,
{
    pub fn new(store: S) -> Self {
        Self {
            store,
            ready_queue: VecDeque::new(),
        }
    }

    pub fn next(&mut self) -> Option<K> {
        self.ready_queue.pop_front()
    }

    pub async fn process(
        &mut self,
        key: K,
        dependencies: Vec<K>,
    ) -> Result<(), DependencyCheckerError> {
        if !self.store.ready(&dependencies).await? {
            // Add pending item to the store.
            self.store.add_pending(key, dependencies).await?;
            return Ok(());
        }

        // Add ready item to the store.
        self.store.add_ready(key).await?;

        // And move it to the ready queue.
        self.ready_queue.push_back(key);

        // Process any pending items which depend on this item.
        self.process_pending(key).await?;

        Ok(())
    }

    // Recursively check if any pending items now have their dependencies met (due to another
    // item being processed).
    async fn process_pending(&mut self, key: K) -> Result<(), DependencyCheckerError> {
        let Some(dependents) = self.store.get_next_pending(key).await? else {
            return Ok(());
        };

        for (next_key, next_deps) in dependents {
            if !self.store.ready(&next_deps).await? {
                continue;
            }

            self.store.add_ready(next_key).await?;

            // And insert this value to the ready_queue.
            self.ready_queue.push_back(next_key);

            // Now check if this item moving into the processed set results in any other
            // items having all their dependencies met.
            Box::pin(self.process_pending(next_key)).await?;
        }

        self.store.remove_pending(key).await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use crate::ordering::dependency_checker::MemoryStore;

    use super::DependencyChecker;

    #[tokio::test]
    async fn dependency_check() {
        // A has no dependencies and so it's added straight to the processed set and ready queue.
        let store = MemoryStore::default();
        let mut checker = DependencyChecker::new(store);
        checker.process("a", vec![]).await.unwrap();
        assert_eq!(checker.store.ready.len(), 1);
        assert_eq!(checker.store.pending.len(), 0);
        assert_eq!(checker.ready_queue.len(), 1);

        // B has it's dependencies met and so it too is added to the processed set and ready queue.
        checker.process("b", vec!["a"]).await.unwrap();
        assert_eq!(checker.store.ready.len(), 2);
        assert_eq!(checker.store.pending.len(), 0);
        assert_eq!(checker.ready_queue.len(), 2);

        // D doesn't have both its dependencies met yet so it waits in the pending queue.
        checker.process("d", vec!["b", "c"]).await.unwrap();
        assert_eq!(checker.store.ready.len(), 2);
        assert_eq!(checker.store.pending.len(), 1);
        assert_eq!(checker.ready_queue.len(), 2);

        // C satisfies D's dependencies and so both C & D are added to the processed set
        // and ready queue.
        checker.process("c", vec!["b"]).await.unwrap();
        assert_eq!(checker.store.ready.len(), 4);
        assert_eq!(checker.store.pending.len(), 0);
        assert_eq!(checker.ready_queue.len(), 4);

        let item = checker.next();
        assert_eq!(item, Some("a"));
        let item = checker.next();
        assert_eq!(item, Some("b"));
        let item = checker.next();
        assert_eq!(item, Some("c"));
        let item = checker.next();
        assert_eq!(item, Some("d"));
        let item = checker.next();
        assert!(item.is_none());
    }

    #[tokio::test]
    async fn recursive_dependency_check() {
        let incomplete_graph = [
            ("a", vec![]),
            ("c", vec!["b"]),
            ("d", vec!["c"]),
            ("e", vec!["d"]),
            ("f", vec!["e"]),
            ("g", vec!["f"]),
        ];

        let store = MemoryStore::default();
        let mut checker = DependencyChecker::new(store);
        for (key, dependencies) in incomplete_graph {
            checker.process(key, dependencies).await.unwrap();
        }
        assert_eq!(checker.store.ready.len(), 1);
        assert_eq!(checker.store.pending.len(), 5);
        assert_eq!(checker.ready_queue.len(), 1);

        let missing_dependency = ("b", vec!["a"]);

        checker
            .process(missing_dependency.0, missing_dependency.1)
            .await
            .unwrap();
        assert_eq!(checker.store.ready.len(), 7);
        assert_eq!(checker.store.pending.len(), 0);
        assert_eq!(checker.ready_queue.len(), 7);

        let item = checker.next();
        assert_eq!(item, Some("a"));
        let item = checker.next();
        assert_eq!(item, Some("b"));
        let item = checker.next();
        assert_eq!(item, Some("c"));
        let item = checker.next();
        assert_eq!(item, Some("d"));
        let item = checker.next();
        assert_eq!(item, Some("e"));
        let item = checker.next();
        assert_eq!(item, Some("f"));
        let item = checker.next();
        assert_eq!(item, Some("g"));
        let item = checker.next();
        assert!(item.is_none());
    }

    #[tokio::test]
    async fn complex_graph() {
        // A <-- B1 <-- C1 <--\
        //   \-- B2 <-- C2 <-- D
        //        \---- C3 <--/
        let incomplete_graph = [
            ("a", vec![]),
            ("b1", vec!["a"]),
            ("c1", vec!["b1"]),
            ("c2", vec!["b2"]),
            ("c3", vec!["b2"]),
            ("d", vec!["c1", "c2", "c3"]),
        ];

        let store = MemoryStore::default();
        let mut checker = DependencyChecker::new(store);
        for (key, dependencies) in incomplete_graph {
            checker.process(key, dependencies).await.unwrap();
        }

        // A1, B1 and C1 have dependencies met and were already processed.
        assert!(checker.store.ready.len() == 3);
        assert_eq!(checker.store.pending.len(), 3);
        assert_eq!(checker.ready_queue.len(), 3);

        let item = checker.next();
        assert_eq!(item, Some("a"));
        let item = checker.next();
        assert_eq!(item, Some("b1"));
        let item = checker.next();
        assert_eq!(item, Some("c1"));
        let item = checker.next();
        assert!(item.is_none());

        // No more ready items.
        assert_eq!(checker.ready_queue.len(), 0);

        // Process the missing item.
        let missing_dependency = ("b2", vec!["a"]);
        checker
            .process(missing_dependency.0, missing_dependency.1)
            .await
            .unwrap();

        // All items have now been processed and new ones are waiting in the ready queue.
        assert_eq!(checker.store.ready.len(), 7);
        assert_eq!(checker.store.pending.len(), 0);
        assert_eq!(checker.ready_queue.len(), 4);

        let mut concurrent_items = HashSet::from(["c2", "c3"]);

        let item = checker.next().unwrap();
        assert_eq!(item, "b2");
        let item = checker.next().unwrap();
        assert!(concurrent_items.remove(item));
        let item = checker.next().unwrap();
        assert!(concurrent_items.remove(item));
        let item = checker.next().unwrap();
        assert_eq!(item, "d");
        let item = checker.next();
        assert!(item.is_none());
    }

    #[tokio::test]
    async fn very_out_of_order() {
        // A <-- B1 <-- C1 <--\
        //   \-- B2 <-- C2 <-- D
        //        \---- C3 <--/
        let out_of_order_graph = [
            ("d", vec!["c1", "c2", "c3"]),
            ("c1", vec!["b1"]),
            ("b1", vec!["a"]),
            ("b2", vec!["a"]),
            ("c3", vec!["b2"]),
            ("c2", vec!["b2"]),
            ("a", vec![]),
        ];

        let store = MemoryStore::default();
        let mut checker = DependencyChecker::new(store);
        for (key, dependencies) in out_of_order_graph {
            checker.process(key, dependencies).await.unwrap();
        }

        assert!(checker.store.ready.len() == 7);
        assert_eq!(checker.store.pending.len(), 0);
        assert_eq!(checker.ready_queue.len(), 7);

        let item = checker.next();
        assert_eq!(item, Some("a"));

        let mut concurrent_items = HashSet::from(["b1", "b2", "c1", "c2", "c3"]);

        let item = checker.next().unwrap();
        assert!(concurrent_items.remove(item));
        let item = checker.next().unwrap();
        assert!(concurrent_items.remove(item));
        let item = checker.next().unwrap();
        assert!(concurrent_items.remove(item));
        let item = checker.next().unwrap();
        assert!(concurrent_items.remove(item));
        let item = checker.next().unwrap();
        assert!(concurrent_items.remove(item));
        let item = checker.next().unwrap();
        assert_eq!(item, "d");
        let item = checker.next();
        assert!(item.is_none());
    }
}
