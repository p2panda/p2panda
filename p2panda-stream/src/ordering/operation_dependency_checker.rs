use std::{collections::HashMap, marker::PhantomData};

use p2panda_core::{Body, Extension, Extensions, Hash, Header, Operation};
use p2panda_store::{LogStore, OperationStore};
use thiserror::Error;

use super::dependency_checker::{
    self, DependencyChecker, DependencyCheckerError, DependencyStore, MemoryStore,
};

pub struct OperationDependencyChecker<L, E, OS, DS> {
    store: OS,
    dependency_checker: DependencyChecker<Hash, DS>,
    operation_cache: HashMap<Hash, Operation<E>>,
    _phantom: PhantomData<(L, E)>,
}

impl<L, E, OS, DS> OperationDependencyChecker<L, E, OS, DS>
where
    OS: OperationStore<L, E> + LogStore<L, E>,
    DS: DependencyStore<Hash>,
    E: Extensions,
{
    pub fn new(store: OS, dependency_checker: DependencyChecker<Hash, DS>) -> Self {
        OperationDependencyChecker {
            store,
            dependency_checker,
            operation_cache: Default::default(),
            _phantom: PhantomData::default(),
        }
    }

    pub async fn process(
        &mut self,
        operation: Operation<E>,
    ) -> Result<(), OperationDependencyCheckerError> {
        let hash = operation.hash;
        let previous = operation.header.previous.clone();
        self.operation_cache.insert(operation.hash, operation);
        self.dependency_checker.process(hash, previous).await?;
        Ok(())
    }

    pub async fn next(&mut self) -> Result<Option<Operation<E>>, OperationDependencyCheckerError> {
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
            .map_err(|err| OperationDependencyCheckerError::StoreError(err.to_string()))?
        {
            let operation = Operation {
                hash: header.hash(),
                header,
                body,
            };
            return Ok(Some(operation));
        } else {
            return Err(OperationDependencyCheckerError::MissingOperation(hash));
        };
    }

    pub fn clear_cache(&mut self) {
        self.operation_cache.clear();
    }
}

#[derive(Debug, Error)]
pub enum OperationDependencyCheckerError {
    #[error(transparent)]
    CheckerError(#[from] DependencyCheckerError),

    #[error("store error: {0}")]
    StoreError(String),

    #[error("processed operation not found in store: {0}")]
    MissingOperation(Hash),
}

#[cfg(test)]
mod tests {
    use p2panda_core::{Header, Operation, PrivateKey};
    use p2panda_store::{MemoryStore, OperationStore};

    use crate::ordering::{
        dependency_checker::{self, DependencyChecker},
        operation_dependency_checker::OperationDependencyChecker,
    };

    #[tokio::test]
    async fn operations_with_previous() {
        let private_key = PrivateKey::new();
        let mut operation_store = MemoryStore::<u64>::default();
        let mut header_0 = Header {
            public_key: private_key.public_key(),
            ..Default::default()
        };
        header_0.sign(&private_key);
        let operation_0 = Operation {
            hash: header_0.hash(),
            header: header_0.clone(),
            body: None,
        };

        let mut header_1 = Header {
            public_key: private_key.public_key(),
            seq_num: 1,
            previous: vec![header_0.hash()],
            ..Default::default()
        };
        header_1.sign(&private_key);
        let operation_1 = Operation {
            hash: header_1.hash(),
            header: header_1.clone(),
            body: None,
        };

        operation_store
            .insert_operation(header_0.hash(), &header_0, None, &header_0.to_bytes(), &0)
            .await
            .unwrap();
        operation_store
            .insert_operation(header_1.hash(), &header_1, None, &header_1.to_bytes(), &0)
            .await
            .unwrap();

        let dependency_store = dependency_checker::MemoryStore::default();
        let dependency_checker = DependencyChecker::new(dependency_store);
        let mut operation_checker =
            OperationDependencyChecker::new(operation_store, dependency_checker);

        let result = operation_checker.process(operation_1.clone()).await;
        assert!(result.is_ok());
        let result = operation_checker.process(operation_0.clone()).await;
        assert!(result.is_ok());

        let next = operation_checker.next().await.unwrap();
        assert_eq!(next.unwrap(), operation_0);
        let next = operation_checker.next().await.unwrap();
        assert_eq!(next.unwrap(), operation_1);
    }
}
