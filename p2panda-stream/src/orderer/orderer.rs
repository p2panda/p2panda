// SPDX-License-Identifier: MIT OR Apache-2.0

use std::marker::PhantomData;

use p2panda_store::orderer::OrdererStore;

/// Causally order items which form a dependency graph.
///
/// Any item in the sequence will be processed after it's dependencies have been processed first.
/// This gives us a **linearised sequence in topological order**.
///
/// This functionality is required when, for example, processing a set of messages where some
/// messages _must_ be processed before others. A set such as this would naturally form a graph
/// structure, each item would have a chain of dependencies. Another example would be a package
/// dependency tree, where a certain package depends on one or many others. In order to understand
/// which order we should install these packages, we need to partially order the set and process
/// them from start to finish.
///
/// Partial ordering means that some items may not be directly comparable. Items in different
/// branches of the graph may not have a direct path between them, and so we don't know "which
/// should come first". In fact, as there is no dependency relation between them, it makes no
/// difference which comes first, and depending on the order items are processed the resulting
/// sequence may look differently (it is a non-deterministic algorithm).
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
/// ### Cycle-detection
///
/// Note that no checks are made for cycles occurring in the graph, this should be validated on
/// another layer.
///
/// ### Ready vs. pending
///
/// Items in the process of being ordered are considered to be in one of two states. They are
/// considered in a "ready" state when all their dependencies have themselves been processed, and
/// in a "pending" state when their dependencies have not yet been processed.
///
/// If an item is in a "pending" state then it is held in a pending queue and if it's dependencies
/// are later processed and "ready", then the so far "pending" item will be moved to the "ready"
/// queue. This processing of pending items recursively checks all pending dependents.
#[derive(Clone, Debug)]
pub struct CausalOrderer<ID, S> {
    pub(super) store: S,
    namespace: String,
    _marker: PhantomData<ID>,
}

impl<ID, S> CausalOrderer<ID, S>
where
    ID: Clone,
    S: OrdererStore<ID>,
{
    pub fn new(store: S) -> Self {
        Self {
            store,
            namespace: "default".to_string(),
            _marker: PhantomData,
        }
    }

    pub fn with_namespace(mut self, namespace: impl AsRef<str>) -> Self {
        self.namespace = namespace.as_ref().to_string();
        self
    }

    /// Pop the next item from the ready queue.
    pub async fn next(&self) -> Result<Option<ID>, S::Error> {
        self.store.take_next_ready(&self.namespace).await
    }

    /// Process a new item which may be in a "ready" or "pending" state.
    pub async fn process(&self, key: ID, dependencies: &[ID]) -> Result<bool, S::Error> {
        if !self.store.ready(&self.namespace, dependencies).await? {
            self.store
                .mark_pending(&self.namespace, key.clone(), dependencies.to_vec())
                .await?;
            return Ok(false);
        }

        self.store.mark_ready(&self.namespace, key.clone()).await?;

        // We added a new ready item to the store so now we want to process any pending items which
        // depend on it as they may now have transitioned into a ready state.
        self.process_pending(key).await?;

        Ok(true)
    }

    /// Recursively check if any pending items now have their dependencies met.
    async fn process_pending(&self, key: ID) -> Result<(), S::Error> {
        // Get all items which depend on the passed key.
        let Some(dependents) = self
            .store
            .get_next_pending(&self.namespace, key.clone())
            .await?
        else {
            return Ok(());
        };

        // For each dependent check if it has all it's dependencies met, if not then we do nothing
        // as it is still in a pending state.
        for (next_key, next_deps) in dependents {
            if !self.store.ready(&self.namespace, &next_deps).await? {
                continue;
            }

            self.store
                .mark_ready(&self.namespace, next_key.clone())
                .await?;

            // Recurse down the dependency graph by now checking any pending items which depend on
            // the current item.
            Box::pin(self.process_pending(next_key)).await?;
        }

        // Finally remove this item from the pending items queue.
        self.store.remove_pending(&self.namespace, key).await?;

        Ok(())
    }
}
