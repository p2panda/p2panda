// SPDX-License-Identifier: MIT OR Apache-2.0

use std::cell::RefCell;
use std::collections::HashMap;
use std::convert::Infallible;
use std::rc::Rc;

use p2panda_core::traits::OperationId;

use crate::memory::MemoryStore;
use crate::operations::OperationStore;

#[allow(clippy::type_complexity)]
#[derive(Clone)]
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
    T: Clone,
    ID: OperationId,
{
    type Error = Infallible;

    async fn insert_operation(&self, id: &ID, operation: T) -> Result<bool, Self::Error> {
        let mut operations = self.operations.operations.borrow_mut();
        Ok(operations.insert(*id, operation).is_none())
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
        Ok(operations.remove(id).is_none())
    }
}
