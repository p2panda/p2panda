// SPDX-License-Identifier: MIT OR Apache-2.0

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::convert::Infallible;
use std::fmt::Debug;
use std::hash::Hash as StdHash;
use std::rc::Rc;

use p2panda_core::logs::StateVector;
use p2panda_core::{Extensions, Hash, Operation, PublicKey};

use crate::memory::MemoryStore;
use crate::operations::{LogStore, OperationStore, SeqNum};

#[allow(clippy::type_complexity)]
#[derive(Clone, Debug)]
pub struct OperationMemoryStore<T, ID> {
    operations: Rc<RefCell<HashMap<ID, T>>>,
}

impl<T, ID> OperationMemoryStore<T, ID> {
    pub fn new() -> Self {
        Self {
            operations: Rc::default(),
        }
    }
}

impl<T, ID> Default for OperationMemoryStore<T, ID> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T, ID> OperationStore<T, ID> for MemoryStore<T, ID>
where
    T: Clone + Debug,
    ID: Clone + Eq + Debug + StdHash,
{
    type Error = Infallible;

    async fn insert_operation(&self, id: &ID, operation: T) -> Result<bool, Self::Error> {
        let mut operations = self.operations.operations.borrow_mut();
        Ok(operations.insert(id.clone(), operation).is_none())
    }

    async fn get_operation(&self, id: &ID) -> Result<Option<T>, Self::Error> {
        let operations = self.operations.operations.borrow();
        Ok(operations.get(id).cloned())
    }

    async fn has_operation(&self, id: &ID) -> Result<bool, Self::Error> {
        let operations = self.operations.operations.borrow();
        Ok(operations.contains_key(id))
    }

    async fn delete_operation(&self, id: &ID) -> Result<bool, Self::Error> {
        let mut operations = self.operations.operations.borrow_mut();
        Ok(operations.remove(id).is_some())
    }

    async fn delete_operation_payload(&self, id: &ID) -> Result<bool, Self::Error> {
        todo!()
    }
}

impl<L, E> LogStore<Operation<E>, L, Hash> for MemoryStore<Operation<E>, Hash>
where
    E: Extensions,
{
    type Error = Infallible;

    async fn has_entry(&self, log_id: &L, id: &Hash, seq_num: SeqNum) -> Result<bool, Self::Error> {
        todo!()
    }

    async fn get_log_height(
        &self,
        public_key: &PublicKey,
        log_id: &L,
    ) -> Result<Option<(Hash, SeqNum)>, Self::Error> {
        todo!()
    }

    async fn get_frontiers(
        &self,
        author: &PublicKey,
        logs: &[L],
    ) -> Result<Option<HashMap<L, StateVector>>, Self::Error> {
        todo!()
    }

    async fn get_log_size(
        &self,
        public_key: &PublicKey,
        log_id: &L,
        after: Option<SeqNum>,
    ) -> Result<Option<(u64, u64)>, Self::Error> {
        todo!()
    }

    async fn get_log_entries(
        &self,
        public_key: &PublicKey,
        log_id: &L,
        after: Option<SeqNum>,
    ) -> Result<Option<Vec<Operation<E>>>, Self::Error> {
        todo!()
    }

    async fn delete_entries(
        &self,
        author: &PublicKey,
        log_id: &L,
        before: &SeqNum,
    ) -> Result<bool, Self::Error> {
        todo!()
    }
}
