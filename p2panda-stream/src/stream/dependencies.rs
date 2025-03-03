use std::{collections::HashMap, marker::PhantomData};

use p2panda_core::{Extensions, Hash, Operation};
use p2panda_store::{LogStore, OperationStore};
use thiserror::Error;

use crate::dependencies::{
    DependencyChecker as InnerDependencyChecker, DependencyCheckerError, DependencyStore,
};

pub struct DependencyChecker<L, E, OS, DS> {
    operation_store: OS,
    inner: InnerDependencyChecker<Hash, DS>,
    operation_cache: HashMap<Hash, Operation<E>>,
    _phantom: PhantomData<(L, E)>,
}

impl<L, E, OS, DS> DependencyChecker<L, E, OS, DS>
where
    OS: OperationStore<L, E> + LogStore<L, E>,
    DS: DependencyStore<Hash>,
    E: Extensions,
{
    pub fn new(operation_store: OS, dependency_store: DS) -> Self {
        let inner_dependency_checker = InnerDependencyChecker::new(dependency_store);
        DependencyChecker {
            operation_store,
            inner: inner_dependency_checker,
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
        self.inner.process(hash, previous).await?;
        Ok(())
    }

    pub async fn next(&mut self) -> Result<Option<Operation<E>>, OperationDependencyCheckerError> {
        let Some(hash) = self.inner.next() else {
            return Ok(None);
        };

        if let Some(operation) = self.operation_cache.remove(&hash) {
            return Ok(Some(operation));
        }

        if let Some((header, body)) = self
            .operation_store
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
    use std::collections::HashSet;

    use p2panda_core::{Header, Operation, PrivateKey};
    use p2panda_store::{MemoryStore, OperationStore};

    use crate::dependencies::MemoryStore as DependencyMemoryStore;

    use super::DependencyChecker;

    #[tokio::test]
    async fn operations_with_previous() {
        let private_key = PrivateKey::new();
        let mut operation_store = MemoryStore::<u64>::default();

        // 0P0 <-- 0P1 <--\
        //     \-- OP2 <-- OP4
        //      \--OP3 <--/

        let mut header_0 = Header {
            public_key: private_key.public_key(),
            timestamp: 0,
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
            previous: vec![header_0.hash()],
            timestamp: 1,
            ..Default::default()
        };
        header_1.sign(&private_key);
        let operation_1 = Operation {
            hash: header_1.hash(),
            header: header_1.clone(),
            body: None,
        };

        let mut header_2 = Header {
            public_key: private_key.public_key(),
            previous: vec![header_0.hash()],
            timestamp: 2,
            ..Default::default()
        };
        header_2.sign(&private_key);
        let operation_2 = Operation {
            hash: header_2.hash(),
            header: header_2.clone(),
            body: None,
        };

        let mut header_3 = Header {
            public_key: private_key.public_key(),
            previous: vec![header_0.hash()],
            timestamp: 3,
            ..Default::default()
        };
        header_3.sign(&private_key);
        let operation_3 = Operation {
            hash: header_3.hash(),
            header: header_3.clone(),
            body: None,
        };

        let mut header_4 = Header {
            public_key: private_key.public_key(),
            previous: vec![header_1.hash(), header_2.hash(), header_3.hash()],
            timestamp: 4,
            ..Default::default()
        };
        header_4.sign(&private_key);
        let operation_4 = Operation {
            hash: header_4.hash(),
            header: header_4.clone(),
            body: None,
        };

        operation_store
            .insert_operation(header_0.hash(), &header_0, None, &header_0.to_bytes(), &0)
            .await
            .unwrap();

        operation_store
            .insert_operation(header_1.hash(), &header_1, None, &header_1.to_bytes(), &1)
            .await
            .unwrap();

        operation_store
            .insert_operation(header_2.hash(), &header_2, None, &header_2.to_bytes(), &2)
            .await
            .unwrap();

        operation_store
            .insert_operation(header_3.hash(), &header_3, None, &header_3.to_bytes(), &3)
            .await
            .unwrap();

        operation_store
            .insert_operation(header_4.hash(), &header_4, None, &header_4.to_bytes(), &4)
            .await
            .unwrap();

        let dependency_store = DependencyMemoryStore::default();
        let mut operation_checker = DependencyChecker::new(operation_store, dependency_store);

        let result = operation_checker.process(operation_4.clone()).await;
        assert!(result.is_ok());
        let result = operation_checker.process(operation_3.clone()).await;
        assert!(result.is_ok());
        let result = operation_checker.process(operation_2.clone()).await;
        assert!(result.is_ok());
        let result = operation_checker.process(operation_1.clone()).await;
        assert!(result.is_ok());
        let result = operation_checker.process(operation_0.clone()).await;
        assert!(result.is_ok());

        let next = operation_checker.next().await.unwrap();
        assert_eq!(next.unwrap(), operation_0);

        let mut concurrent_operations =
            HashSet::from([operation_1.hash, operation_2.hash, operation_3.hash]);

        let next = operation_checker.next().await.unwrap();
        assert!(next.is_some());
        let next = next.unwrap();
        assert!(concurrent_operations.remove(&next.hash),);

        let next = operation_checker.next().await.unwrap();
        assert!(next.is_some());
        let next = next.unwrap();
        assert!(concurrent_operations.remove(&next.hash),);

        let next = operation_checker.next().await.unwrap();
        assert!(next.is_some());
        let next = next.unwrap();
        assert!(concurrent_operations.remove(&next.hash),);

        let next = operation_checker.next().await.unwrap();
        assert_eq!(next.unwrap(), operation_4);
    }
}
