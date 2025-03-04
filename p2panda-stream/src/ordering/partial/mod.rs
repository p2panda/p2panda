// SPDX-License-Identifier: MIT OR Apache-2.0

pub mod store;

use std::collections::VecDeque;
use std::fmt::{Debug, Display};
use std::hash::Hash as StdHash;

use thiserror::Error;

use store::PartialOrderStore;

/// Error types which may be returned from `PartialOrder` methods.
#[derive(Debug, Error)]
pub enum PartialOrderError {
    #[error("store error: {0}")]
    StoreError(String),
}

/// Struct for establishing partial order over a Directed-Acyclic-Graph.
///
/// There are various approaches which can be taken when wanting to linearize items in a graph
/// structure. This approach establishes a partial order, meaning not all items in the graph are
/// comparable, and is non-deterministic. The main requirement is that all dependencies of an item
/// are sorted "before" the item itself, the exact order is not a concern.
///
/// Example graph:
///
/// A <-- B2 <-- C
///   \-- B1 <--/
///
/// Both of the following are possible and valid orderings for the above graph:
///
/// [A, B1, B2, C]
/// [A, B2, B1, C]
///
/// Items will not be placed into an partial order until all their dependencies are met, in the
/// following example item C will not be visited as we have not processed all of it's
/// dependencies.
///
/// Example graph:
///
/// A <-- ?? <-- C
///   \-- B1 <--/
///
/// C is not processed yet as we are missing one of its dependencies:
///
/// [A, B1]
///
#[derive(Debug)]
pub struct PartialOrder<K, S> {
    store: S,
    ready_queue: VecDeque<K>,
}

impl<K, S> PartialOrder<K, S>
where
    K: Clone + Copy + Display + StdHash + PartialEq + Eq,
    S: PartialOrderStore<K>,
{
    pub fn new(store: S) -> Self {
        Self {
            store,
            ready_queue: VecDeque::new(),
        }
    }

    /// Pop the next item from the ready queue.
    pub fn next(&mut self) -> Option<K> {
        self.ready_queue.pop_front()
    }

    /// Process a new item which may be in a "ready" or "pending" state.
    pub async fn process(&mut self, key: K, dependencies: Vec<K>) -> Result<(), PartialOrderError> {
        if !self.store.ready(&dependencies).await? {
            self.store.add_pending(key, dependencies).await?;
            return Ok(());
        }

        self.store.add_ready(key).await?;
        self.ready_queue.push_back(key);

        // We added a new ready item to the store so now we want to process any pending items
        // which depend on it as they may now have transitioned into a ready state.
        self.process_pending(key).await?;

        Ok(())
    }

    /// Recursively check if any pending items now have their dependencies met.
    async fn process_pending(&mut self, key: K) -> Result<(), PartialOrderError> {
        // Get all items which depend on the passed key.
        let Some(dependents) = self.store.get_next_pending(key).await? else {
            return Ok(());
        };

        // For each dependent check if it has all it's dependencies met, if not then we do nothing
        // as it is still in a pending state.
        for (next_key, next_deps) in dependents {
            if !self.store.ready(&next_deps).await? {
                continue;
            }

            self.store.add_ready(next_key).await?;
            self.ready_queue.push_back(next_key);

            // Recurse down the dependency graph by now checking any pending items which depend on
            // the current item.
            Box::pin(self.process_pending(next_key)).await?;
        }

        // Finally remove this item from the pending items queue.
        self.store.remove_pending(key).await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use crate::ordering::MemoryStore;

    use super::PartialOrder;

    #[tokio::test]
    async fn partial_order() {
        // Graph
        //
        // A <-- B <--------- D
        //        \--- C <---/
        //
        let graph = [
            ("A", vec![]),
            ("B", vec!["A"]),
            ("C", vec!["B"]),
            ("D", vec!["B", "C"]),
        ];

        // A has no dependencies and so it's added straight to the processed set and ready queue.
        let store = MemoryStore::default();
        let mut checker = PartialOrder::new(store);
        let item = graph[0].clone();
        checker.process(item.0, item.1).await.unwrap();
        assert_eq!(checker.store.ready.len(), 1);
        assert_eq!(checker.store.pending.len(), 0);
        assert_eq!(checker.ready_queue.len(), 1);

        // B has it's dependencies met and so it too is added to the processed set and ready
        // queue.
        let item = graph[1].clone();
        checker.process(item.0, item.1).await.unwrap();
        assert_eq!(checker.store.ready.len(), 2);
        assert_eq!(checker.store.pending.len(), 0);
        assert_eq!(checker.ready_queue.len(), 2);

        // D doesn't have both its dependencies met yet so it waits in the pending queue.
        let item = graph[3].clone();
        checker.process(item.0, item.1).await.unwrap();
        assert_eq!(checker.store.ready.len(), 2);
        assert_eq!(checker.store.pending.len(), 1);
        assert_eq!(checker.ready_queue.len(), 2);

        // C satisfies D's dependencies and so both C & D are added to the processed set
        // and ready queue.
        let item = graph[2].clone();
        checker.process(item.0, item.1).await.unwrap();
        assert_eq!(checker.store.ready.len(), 4);
        assert_eq!(checker.store.pending.len(), 0);
        assert_eq!(checker.ready_queue.len(), 4);

        let item = checker.next();
        assert_eq!(item, Some("A"));
        let item = checker.next();
        assert_eq!(item, Some("B"));
        let item = checker.next();
        assert_eq!(item, Some("C"));
        let item = checker.next();
        assert_eq!(item, Some("D"));
        let item = checker.next();
        assert!(item.is_none());
    }

    #[tokio::test]
    async fn partial_order_with_recursion() {
        // Graph
        //
        // A <-- B <--------- D
        //        \--- C <---/
        //
        let incomplete_graph = [
            ("A", vec![]),
            ("C", vec!["B"]),
            ("D", vec!["C"]),
            ("E", vec!["D"]),
            ("F", vec!["E"]),
            ("G", vec!["F"]),
        ];

        let store = MemoryStore::default();
        let mut checker = PartialOrder::new(store);
        for (key, dependencies) in incomplete_graph {
            checker.process(key, dependencies).await.unwrap();
        }
        assert_eq!(checker.store.ready.len(), 1);
        assert_eq!(checker.store.pending.len(), 5);
        assert_eq!(checker.ready_queue.len(), 1);

        let missing_dependency = ("B", vec!["A"]);

        checker
            .process(missing_dependency.0, missing_dependency.1)
            .await
            .unwrap();
        assert_eq!(checker.store.ready.len(), 7);
        assert_eq!(checker.store.pending.len(), 0);
        assert_eq!(checker.ready_queue.len(), 7);

        let item = checker.next();
        assert_eq!(item, Some("A"));
        let item = checker.next();
        assert_eq!(item, Some("B"));
        let item = checker.next();
        assert_eq!(item, Some("C"));
        let item = checker.next();
        assert_eq!(item, Some("D"));
        let item = checker.next();
        assert_eq!(item, Some("E"));
        let item = checker.next();
        assert_eq!(item, Some("F"));
        let item = checker.next();
        assert_eq!(item, Some("G"));
        let item = checker.next();
        assert!(item.is_none());
    }

    #[tokio::test]
    async fn complex_graph() {
        // Graph
        //
        // A <-- B1 <-- C1 <--\
        //   \-- ?? <-- C2 <-- D
        //        \---- C3 <--/
        //
        let incomplete_graph = [
            ("A", vec![]),
            ("B1", vec!["A"]),
            // This item is missing.
            // ("B2", vec!["A"]),
            ("C1", vec!["B1"]),
            ("C2", vec!["B2"]),
            ("C3", vec!["B2"]),
            ("D", vec!["C1", "C2", "C3"]),
        ];

        let store = MemoryStore::default();
        let mut checker = PartialOrder::new(store);
        for (key, dependencies) in incomplete_graph {
            checker.process(key, dependencies).await.unwrap();
        }

        // A1, B1 and C1 have dependencies met and were already processed.
        assert!(checker.store.ready.len() == 3);
        assert_eq!(checker.store.pending.len(), 3);
        assert_eq!(checker.ready_queue.len(), 3);

        let item = checker.next();
        assert_eq!(item, Some("A"));
        let item = checker.next();
        assert_eq!(item, Some("B1"));
        let item = checker.next();
        assert_eq!(item, Some("C1"));
        let item = checker.next();
        assert!(item.is_none());

        // No more ready items.
        assert_eq!(checker.ready_queue.len(), 0);

        // Process the missing item.
        let missing_dependency = ("B2", vec!["A"]);
        checker
            .process(missing_dependency.0, missing_dependency.1)
            .await
            .unwrap();

        // All items have now been processed and new ones are waiting in the ready queue.
        assert_eq!(checker.store.ready.len(), 7);
        assert_eq!(checker.store.pending.len(), 0);
        assert_eq!(checker.ready_queue.len(), 4);

        let mut concurrent_items = HashSet::from(["C2", "C3"]);

        let item = checker.next().unwrap();
        assert_eq!(item, "B2");
        let item = checker.next().unwrap();
        assert!(concurrent_items.remove(item));
        let item = checker.next().unwrap();
        assert!(concurrent_items.remove(item));
        let item = checker.next().unwrap();
        assert_eq!(item, "D");
        let item = checker.next();
        assert!(item.is_none());
    }

    #[tokio::test]
    async fn very_out_of_order() {
        // Graph
        //
        // A <-- B1 <-- C1 <--\
        //   \-- B2 <-- C2 <-- D
        //        \---- C3 <--/
        //
        let out_of_order_graph = [
            ("D", vec!["C1", "C2", "C3"]),
            ("C1", vec!["B1"]),
            ("B1", vec!["A"]),
            ("B2", vec!["A"]),
            ("C3", vec!["B2"]),
            ("C2", vec!["B2"]),
            ("A", vec![]),
        ];

        let store = MemoryStore::default();
        let mut checker = PartialOrder::new(store);
        for (key, dependencies) in out_of_order_graph {
            checker.process(key, dependencies).await.unwrap();
        }

        assert!(checker.store.ready.len() == 7);
        assert_eq!(checker.store.pending.len(), 0);
        assert_eq!(checker.ready_queue.len(), 7);

        let item = checker.next();
        assert_eq!(item, Some("A"));

        let mut concurrent_items = HashSet::from(["B1", "B2", "C1", "C2", "C3"]);

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
        assert_eq!(item, "D");
        let item = checker.next();
        assert!(item.is_none());
    }
}
