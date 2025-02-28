use std::{collections::HashMap, marker::PhantomData};

use p2panda_core::{Body, Extension, Extensions, Hash, Header, Operation};
use p2panda_store::{LogStore, OperationStore};
use thiserror::Error;

use super::dependency_checker::{DependencyChecker, MemoryStore};

pub struct OperationDependencyChecker<L, E, S> {
    store: S,
    dependency_checker: DependencyChecker<Hash, MemoryStore<Hash>>,
    operation_cache: HashMap<Hash, Operation<E>>,
    _phantom: PhantomData<(L, E)>,
}

impl<L, E, S> OperationDependencyChecker<L, E, S>
where
    S: OperationStore<L, E> + LogStore<L, E>,
    E: Extension<L> + Extensions,
{
    pub async fn process(&mut self, operation: Operation<E>) {
        let hash = operation.hash;
        let previous = operation.header.previous.clone();

        self.operation_cache.insert(operation.hash, operation);

        self.dependency_checker.process(hash, previous);
    }

    pub async fn next(&mut self) -> Result<Option<Operation<E>>, DependencyCheckerError> {
        let Some(hash) = self.dependency_checker.next() else {
            return Ok(None);
        };

        if let Some(operation) = self.operation_cache.remove(&hash) {
            return Ok(Some(operation));
        }

        if let Some((header, body)) = self
            .store
            .get_operation(hash)
            .await
            .map_err(|err| DependencyCheckerError::StoreError(err.to_string()))?
        {
            let operation = Operation {
                hash: header.hash(),
                header,
                body,
            };
            return Ok(Some(operation));
        } else {
            return Err(DependencyCheckerError::MissingOperation(hash));
        };
    }

    pub fn clear_cache(&mut self) {
        self.operation_cache.clear();
    }
}

#[derive(Debug, Error)]
pub enum DependencyCheckerError {
    #[error("store error: {0}")]
    StoreError(String),

    #[error("processed operation not found in store: {0}")]
    MissingOperation(Hash),
}
