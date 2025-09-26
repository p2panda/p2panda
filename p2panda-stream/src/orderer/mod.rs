// SPDX-License-Identifier: MIT OR Apache-2.0

use std::marker::PhantomData;

// @TODO: Change this to p2panda_store when ready.
use p2panda_store_next::orderer::OrdererStore;

/// Struct for establishing partial order over a set of items which form a dependency graph.
///
/// A partial order sorts items based on their causal relationships. An item can be "before",
/// "after" or "at the same time" as any other item.
///
/// This functionality is required when, for example, processing a set of messages where some
/// messages _must_ be processed before others. A set such as this would naturally form a graph
/// structure, each item would have a chain of dependencies. Another example would be a package
/// dependency tree, where a certain package depends on one or many others. In order to understand
/// which order we should install these packages, we need to partially order the set and process
/// them from start to finish.
///
/// There are various approaches which can be taken when wanting to linearize items in a graph
/// structure. The approach taken in this module establishes a partial order over all items in the
/// set. The word "partial" indicates that some items may not be directly comparable. Items in
/// different branches of the graph may not have a direct path between them, and so we don't know
/// "which should come first". In fact, as there is no dependency relation between them, it makes
/// no difference which comes first, and depending on the order items are processed the ordering
/// process may arrive at different results (it is a non-deterministic algorithm).
///
/// Items in the process of being ordered are considered to be in one of two states. They are
/// considered in a "ready" state when all their dependencies have themselves been processed, and
/// in a "pending" state when their dependencies have not yet been processed.
///
/// If an item is in a "pending" state then it is held in a pending queue and if it's dependencies
/// are later processed and "ready", then the so far "pending" item will be moved to the "ready"
/// queue. This processing of pending items recursively checks all pending dependents.
///
/// Example graph:
///
/// ```text
/// A <-- B2 <-- C
///   \-- B1 <--/
/// ```
///
/// Both of the following are possible and valid orderings for the above graph:
///
/// ```text
/// [A, B1, B2, C]
/// [A, B2, B1, C]
/// ```
///
/// Items will not be placed into an partial order until all their dependencies are met, in the
/// following example item C will not be visited as we have not processed all of it's
/// dependencies.
///
/// Example graph:
///
/// ```text
/// A <-- ?? <-- C
///  \-- B1 <--/
/// ```
///
/// C is not processed yet as we are missing one of its dependencies:
///
/// ```text
/// [A, B1]
/// ```
///
/// Note that no checks are made for cycles occurring in the graph, this should be validated on
/// another layer.
#[derive(Debug)]
pub struct PartialOrder<ID, S> {
    /// Store for managing "ready" and "pending" items.
    store: S,
    _phantom: PhantomData<ID>,
}

impl<ID, S> PartialOrder<ID, S>
where
    ID: Copy,
    S: OrdererStore<ID>,
{
    pub fn new(store: S) -> Self {
        Self {
            store,
            _phantom: PhantomData,
        }
    }

    /// Pop the next item from the ready queue.
    pub async fn next(&mut self) -> Result<Option<ID>, S::Error> {
        self.store.take_next_ready().await
    }

    /// Process a new item which may be in a "ready" or "pending" state.
    pub async fn process(&mut self, key: ID, dependencies: &[ID]) -> Result<(), S::Error> {
        if !self.store.ready(dependencies).await? {
            self.store.mark_pending(key, dependencies.to_vec()).await?;
            return Ok(());
        }

        self.store.mark_ready(key).await?;

        // We added a new ready item to the store so now we want to process any pending items
        // which depend on it as they may now have transitioned into a ready state.
        self.process_pending(key).await?;

        Ok(())
    }

    /// Recursively check if any pending items now have their dependencies met.
    async fn process_pending(&mut self, key: ID) -> Result<(), S::Error> {
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

            self.store.mark_ready(next_key).await?;

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

    // @TODO: Change this to p2panda_store when ready.
    use p2panda_store_next::memory::MemoryStore;
    use p2panda_store_next::orderer::OrdererTestExt;

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
        let store = MemoryStore::<(), &'static str>::default();
        let mut checker = PartialOrder::new(store);
        let item = graph[0].clone();
        checker.process(item.0, &item.1).await.unwrap();
        assert_eq!(checker.store.ready_len(), 1);
        assert_eq!(checker.store.pending_len(), 0);
        assert_eq!(checker.store.ready_queue_len(), 1);

        // B has it's dependencies met and so it too is added to the processed set and ready
        // queue.
        let item = graph[1].clone();
        checker.process(item.0, &item.1).await.unwrap();
        assert_eq!(checker.store.ready_len(), 2);
        assert_eq!(checker.store.pending_len(), 0);
        assert_eq!(checker.store.ready_queue_len(), 2);

        // D doesn't have both its dependencies met yet so it waits in the pending queue.
        let item = graph[3].clone();
        checker.process(item.0, &item.1).await.unwrap();
        assert_eq!(checker.store.ready_len(), 2);
        assert_eq!(checker.store.pending_len(), 1);
        assert_eq!(checker.store.ready_queue_len(), 2);

        // C satisfies D's dependencies and so both C & D are added to the processed set
        // and ready queue.
        let item = graph[2].clone();
        checker.process(item.0, &item.1).await.unwrap();
        assert_eq!(checker.store.ready_len(), 4);
        assert_eq!(checker.store.pending_len(), 0);
        assert_eq!(checker.store.ready_queue_len(), 4);

        let item = checker.next().await.unwrap();
        assert_eq!(item, Some("A"));
        let item = checker.next().await.unwrap();
        assert_eq!(item, Some("B"));
        let item = checker.next().await.unwrap();
        assert_eq!(item, Some("C"));
        let item = checker.next().await.unwrap();
        assert_eq!(item, Some("D"));
        let item = checker.next().await.unwrap();
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

        let store = MemoryStore::<(), &'static str>::default();
        let mut checker = PartialOrder::new(store);
        for (key, dependencies) in incomplete_graph {
            checker.process(key, &dependencies).await.unwrap();
        }
        assert_eq!(checker.store.ready_len(), 1);
        assert_eq!(checker.store.pending_len(), 5);
        assert_eq!(checker.store.ready_queue_len(), 1);

        let missing_dependency = ("B", vec!["A"]);

        checker
            .process(missing_dependency.0, &missing_dependency.1)
            .await
            .unwrap();
        assert_eq!(checker.store.ready_len(), 7);
        assert_eq!(checker.store.pending_len(), 0);
        assert_eq!(checker.store.ready_queue_len(), 7);

        let item = checker.next().await.unwrap();
        assert_eq!(item, Some("A"));
        let item = checker.next().await.unwrap();
        assert_eq!(item, Some("B"));
        let item = checker.next().await.unwrap();
        assert_eq!(item, Some("C"));
        let item = checker.next().await.unwrap();
        assert_eq!(item, Some("D"));
        let item = checker.next().await.unwrap();
        assert_eq!(item, Some("E"));
        let item = checker.next().await.unwrap();
        assert_eq!(item, Some("F"));
        let item = checker.next().await.unwrap();
        assert_eq!(item, Some("G"));
        let item = checker.next().await.unwrap();
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

        let store = MemoryStore::<(), &'static str>::default();
        let mut checker = PartialOrder::new(store);
        for (key, dependencies) in incomplete_graph {
            checker.process(key, &dependencies).await.unwrap();
        }

        // A1, B1 and C1 have dependencies met and were already processed.
        assert!(checker.store.ready_len() == 3);
        assert_eq!(checker.store.pending_len(), 3);
        assert_eq!(checker.store.ready_queue_len(), 3);

        let item = checker.next().await.unwrap();
        assert_eq!(item, Some("A"));
        let item = checker.next().await.unwrap();
        assert_eq!(item, Some("B1"));
        let item = checker.next().await.unwrap();
        assert_eq!(item, Some("C1"));
        let item = checker.next().await.unwrap();
        assert!(item.is_none());

        // No more ready items.
        assert_eq!(checker.store.ready_queue_len(), 0);

        // Process the missing item.
        let missing_dependency = ("B2", vec!["A"]);
        checker
            .process(missing_dependency.0, &missing_dependency.1)
            .await
            .unwrap();

        // All items have now been processed and new ones are waiting in the ready queue.
        assert_eq!(checker.store.ready_len(), 7);
        assert_eq!(checker.store.pending_len(), 0);
        assert_eq!(checker.store.ready_queue_len(), 4);

        let mut concurrent_items = HashSet::from(["C2", "C3"]);

        let item = checker.next().await.unwrap().unwrap();
        assert_eq!(item, "B2");
        let item = checker.next().await.unwrap().unwrap();
        assert!(concurrent_items.remove(item));
        let item = checker.next().await.unwrap().unwrap();
        assert!(concurrent_items.remove(item));
        let item = checker.next().await.unwrap().unwrap();
        assert_eq!(item, "D");
        let item = checker.next().await.unwrap();
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

        let store = MemoryStore::<(), &'static str>::default();
        let mut checker = PartialOrder::new(store);
        for (key, dependencies) in out_of_order_graph {
            checker.process(key, &dependencies).await.unwrap();
        }

        assert!(checker.store.ready_len() == 7);
        assert_eq!(checker.store.pending_len(), 0);
        assert_eq!(checker.store.ready_queue_len(), 7);

        let item = checker.next().await.unwrap();
        assert_eq!(item, Some("A"));

        let mut concurrent_items = HashSet::from(["B1", "B2", "C1", "C2", "C3"]);

        let item = checker.next().await.unwrap().unwrap();
        assert!(concurrent_items.remove(item));
        let item = checker.next().await.unwrap().unwrap();
        assert!(concurrent_items.remove(item));
        let item = checker.next().await.unwrap().unwrap();
        assert!(concurrent_items.remove(item));
        let item = checker.next().await.unwrap().unwrap();
        assert!(concurrent_items.remove(item));
        let item = checker.next().await.unwrap().unwrap();
        assert!(concurrent_items.remove(item));
        let item = checker.next().await.unwrap().unwrap();
        assert_eq!(item, "D");
        let item = checker.next().await.unwrap();
        assert!(item.is_none());
    }
}
