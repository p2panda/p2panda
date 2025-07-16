// SPDX-License-Identifier: MIT OR Apache-2.0

use std::convert::Infallible;
use std::sync::Arc;

use p2panda_store::{Transaction, WritableStore, WriteToStore};
use tokio::sync::RwLock;

// @TODO(adz): Parts of this will be moved to `p2panda-store` eventually (next to `SqliteStore`).
// These are concrete storage implementations which can be used across our whole stack. The
// required changes are:
//
// 1. Move concrete database implementations into `p2panda-store`
// 2. Remove "write" methods from all current store traits, like `OperationStore` and `LogStore`
// 3. Separate code nicely so database impls are not related to trait impls
// 4. Decide where to move trait impls like `OperationStore` etc. (should that be in core?)
// 5. Implement `WriteToStore` for `Operation` etc.

/// Thread-safe in-memory store backend for p2panda.
///
/// This implements the "atomic transaction" API rather naively by cloning _all_ state into the
/// transaction object, where it is mutated until it finally overrides the current state on
/// "commit". Since this approach is not memory-efficient this store should only be used for
/// development and testing purposes.
#[derive(Clone)]
pub struct MemoryStore<T> {
    state: Arc<RwLock<T>>,
}

impl<T> MemoryStore<T> {
    pub fn new(initial_state: T) -> Self {
        MemoryStore {
            state: Arc::new(RwLock::new(initial_state)),
        }
    }
}

impl<T> WritableStore for MemoryStore<T>
where
    T: Clone,
{
    type Error = Infallible;

    type Transaction<'c> = MemoryTransaction<T>;

    async fn begin<'c>(&mut self) -> Result<Self::Transaction<'c>, Self::Error> {
        // Start the transaction by cloning the current state which can then be mutated by any
        // process.
        //
        // This is very memory in-efficient and the reason why this implementations should only be
        // used for development & testing purposes.
        //
        // @TODO: if we want to protect against two transactions being open at the same time and
        // operating on independent states (then over-writing each other on commit), we could
        // introduce a semaphore here which only unlocks once commit has been called.
        let current_state = self.state.read().await;
        let next_state = current_state.clone();

        // Keep a reference to the store itself for later. Note that this is ref-counted and not
        // cloning the whole state again.
        let store = self.clone();

        Ok(MemoryTransaction { next_state, store })
    }
}

pub struct MemoryTransaction<T> {
    next_state: T,
    store: MemoryStore<T>,
}

impl<T> Transaction for MemoryTransaction<T> {
    type Error = Infallible;

    async fn commit(self) -> Result<(), Self::Error> {
        let mut current_state = self.store.state.write().await;
        *current_state = self.next_state;
        Ok(())
    }

    async fn rollback(self) -> Result<(), Self::Error> {
        // Consume the object with the (mutated) state. It will get dropped alongside the
        // transaction instance itself. The previous state remains untouched and we don't have
        // anything to do anymore for "rolling back".
        Ok(())
    }
}

/// In-memory state representing _all_ p2panda systems (operations, encryption, auth, orderer, etc.).
#[derive(Clone, Default)]
pub struct AllState {
    spaces: SpacesState,
    orderer: OrdererState,
}

// Read-only traits.

/// Read-only interfaces for retrieving data from p2panda "spaces".
pub trait SpacesStore {
    type Error;

    // @NOTE(adz): When tightly connected to the manager we should watch out for too much stuff living
    // in memory during runtime. If there's anything we can shift to the database instead we should
    // separate the "full state" representation (in an "in memory database" or SQL table) from the
    // committable "diff" we want to write to a database. For now we can keep them identical ("what
    // gets committed to database" and "what is used in the manager") and decide later if we need to
    // separate.
    fn spaces_state(&self) -> impl Future<Output = Result<SpacesState, Self::Error>>;

    // @TODO: This is just an example. Remove this.
    fn current_example_value(&self) -> impl Future<Output = Result<usize, Self::Error>>;
}

impl SpacesStore for MemoryStore<AllState> {
    type Error = Infallible;

    async fn spaces_state(&self) -> Result<SpacesState, Self::Error> {
        let state = self.state.read().await;
        Ok(state.spaces.clone())
    }

    async fn current_example_value(&self) -> Result<usize, Self::Error> {
        let state = self.state.read().await;
        Ok(state.spaces.example)
    }
}

// Store implementations for `p2panda-spaces` for in-memory backends.

#[derive(Clone, Default)]
pub struct SpacesState {
    // @TODO: This is just an example. Remove this.
    example: usize,
}

impl WriteToStore<MemoryStore<AllState>> for SpacesState {
    async fn write(
        &self,
        tx: &mut <MemoryStore<AllState> as WritableStore>::Transaction<'_>,
    ) -> Result<(), Infallible> {
        tx.next_state.spaces = self.clone();
        Ok(())
    }
}

// .. more examples

#[derive(Clone, Default)]
pub struct OrdererState {}

impl WriteToStore<MemoryStore<AllState>> for OrdererState {
    async fn write(
        &self,
        tx: &mut <MemoryStore<AllState> as WritableStore>::Transaction<'_>,
    ) -> Result<(), Infallible> {
        tx.next_state.orderer = self.clone();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use p2panda_store::{Transaction, WritableStore, WriteToStore};

    use crate::store::SpacesStore;

    use super::{AllState, MemoryStore};

    #[tokio::test]
    async fn memory_storage_transaction_example() {
        let mut store = MemoryStore::new(AllState::default());

        assert_eq!(store.current_example_value().await, Ok(0));

        let Ok(mut spaces_state) = store.spaces_state().await;
        spaces_state.example = 12;

        let mut tx = store.begin().await.unwrap();
        spaces_state.write(&mut tx).await.unwrap();

        assert_eq!(store.current_example_value().await, Ok(0));

        tx.commit().await.unwrap();

        assert_eq!(store.current_example_value().await, Ok(12));
    }
}
