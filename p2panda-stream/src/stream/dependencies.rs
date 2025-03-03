use std::{collections::HashMap, marker::PhantomData};

use p2panda_core::{Extensions, Hash, Operation};
use p2panda_store::{LogStore, OperationStore};
use thiserror::Error;

use crate::dependencies::{
    DependencyChecker as InnerDependencyChecker, DependencyCheckerError, DependencyStore,
};

/// Dependency checker which handles p2panda operations.
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
    /// Construct a new dependency checker from an operation store and dependency store.
    pub fn new(operation_store: OS, dependency_store: DS) -> Self {
        let inner_dependency_checker = InnerDependencyChecker::new(dependency_store);
        DependencyChecker {
            operation_store,
            inner: inner_dependency_checker,
            operation_cache: Default::default(),
            _phantom: PhantomData,
        }
    }

    /// Process a single operation.
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

    /// Take the next ready operation from the queue.
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
            Ok(Some(operation))
        } else {
            Err(OperationDependencyCheckerError::MissingOperation(hash))
        }
    }

    /// Clear the operation cache.
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

    /// Create operations which form the following graph with their previous links:
    ///
    /// 0P0 <-- 0P1 <--\
    ///     \-- OP2 <-- OP4
    ///      \--OP3 <--/
    ///
    /// Each operation is inserted into the store in it's own log.
    async fn setup(
        private_key: &PrivateKey,
        operation_store: &mut MemoryStore<u64>,
    ) -> Vec<Operation> {
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

        vec![
            operation_0,
            operation_1,
            operation_2,
            operation_3,
            operation_4,
        ]
    }

    #[tokio::test]
    async fn operations_with_previous() {
        let private_key = PrivateKey::new();
        let mut operation_store = MemoryStore::<u64>::default();
        let dependency_store = DependencyMemoryStore::default();
        let mut dependency_checker =
            DependencyChecker::new(operation_store.clone(), dependency_store);

        // Setup test data in the store. They form a dependency graph with the following form:
        //
        // 0P0 <-- 0P1 <--\
        //     \-- OP2 <-- OP4
        //      \--OP3 <--/
        //
        let operations = setup(&private_key, &mut operation_store).await;

        // Process each operation out-of-order.
        let result = dependency_checker.process(operations[4].clone()).await;
        assert!(result.is_ok());
        // Each operation gets inserted to the operation cache.
        assert_eq!(dependency_checker.operation_cache.len(), 1);

        let result = dependency_checker.process(operations[3].clone()).await;
        assert!(result.is_ok());
        assert_eq!(dependency_checker.operation_cache.len(), 2);

        let result = dependency_checker.process(operations[2].clone()).await;
        assert!(result.is_ok());
        assert_eq!(dependency_checker.operation_cache.len(), 3);

        let result = dependency_checker.process(operations[1].clone()).await;
        assert!(result.is_ok());
        assert_eq!(dependency_checker.operation_cache.len(), 4);

        let result = dependency_checker.process(operations[0].clone()).await;
        assert!(result.is_ok());
        assert_eq!(dependency_checker.operation_cache.len(), 5);

        // Calling next should give us the first operation which had all it's dependencies met, in
        // this case it's the root of the graph OP1.
        let next = dependency_checker.next().await.unwrap();
        assert_eq!(next.unwrap(), operations[0]);

        // Operations OP1, OP2 and OP3 all only depend on operation OP0 and they were created
        // concurrently, we should see all of these operations next, but we don't know in what order.
        let mut concurrent_operations =
            HashSet::from([operations[1].hash, operations[2].hash, operations[3].hash]);

        let next = dependency_checker.next().await.unwrap();
        assert!(next.is_some());
        let next = next.unwrap();
        assert!(concurrent_operations.remove(&next.hash),);

        let next = dependency_checker.next().await.unwrap();
        assert!(next.is_some());
        let next = next.unwrap();
        assert!(concurrent_operations.remove(&next.hash),);

        let next = dependency_checker.next().await.unwrap();
        assert!(next.is_some());
        let next = next.unwrap();
        assert!(concurrent_operations.remove(&next.hash),);

        // We know OP4 will be given last as it depended on OP1, OP2 and OP3.
        let next = dependency_checker.next().await.unwrap();
        assert_eq!(next.unwrap(), operations[4]);
    }

    #[tokio::test]
    async fn clear_cache() {
        // Same test as above except we clear the cache before taking operations via the `next` method.
        let private_key = PrivateKey::new();
        let mut operation_store = MemoryStore::<u64>::default();
        let dependency_store = DependencyMemoryStore::default();
        let mut dependency_checker =
            DependencyChecker::new(operation_store.clone(), dependency_store);

        let operations = setup(&private_key, &mut operation_store).await;

        let result = dependency_checker.process(operations[4].clone()).await;
        assert!(result.is_ok());
        assert_eq!(dependency_checker.operation_cache.len(), 1);
        let result = dependency_checker.process(operations[3].clone()).await;
        assert!(result.is_ok());
        assert_eq!(dependency_checker.operation_cache.len(), 2);
        let result = dependency_checker.process(operations[2].clone()).await;
        assert!(result.is_ok());
        assert_eq!(dependency_checker.operation_cache.len(), 3);
        let result = dependency_checker.process(operations[1].clone()).await;
        assert!(result.is_ok());
        assert_eq!(dependency_checker.operation_cache.len(), 4);
        let result = dependency_checker.process(operations[0].clone()).await;
        assert!(result.is_ok());
        assert_eq!(dependency_checker.operation_cache.len(), 5);

        // Clear the cache!
        dependency_checker.clear_cache();
        assert_eq!(dependency_checker.operation_cache.len(), 0);

        let next = dependency_checker.next().await.unwrap();
        assert_eq!(next.unwrap(), operations[0]);

        let mut concurrent_operations =
            HashSet::from([operations[1].hash, operations[2].hash, operations[3].hash]);

        let next = dependency_checker.next().await.unwrap();
        assert!(next.is_some());
        let next = next.unwrap();
        assert!(concurrent_operations.remove(&next.hash),);

        let next = dependency_checker.next().await.unwrap();
        assert!(next.is_some());
        let next = next.unwrap();
        assert!(concurrent_operations.remove(&next.hash),);

        let next = dependency_checker.next().await.unwrap();
        assert!(next.is_some());
        let next = next.unwrap();
        assert!(concurrent_operations.remove(&next.hash),);

        let next = dependency_checker.next().await.unwrap();
        assert_eq!(next.unwrap(), operations[4]);
    }

    #[tokio::test]
    async fn missing_dependency() {
        let private_key = PrivateKey::new();
        let mut operation_store = MemoryStore::<u64>::default();
        let dependency_store = DependencyMemoryStore::default();
        let mut dependency_checker =
            DependencyChecker::new(operation_store.clone(), dependency_store);

        let operations = setup(&private_key, &mut operation_store).await;

        let result = dependency_checker.process(operations[1].clone()).await;
        assert!(result.is_ok());
        assert_eq!(dependency_checker.operation_cache.len(), 1);
        let result = dependency_checker.process(operations[2].clone()).await;
        assert!(result.is_ok());
        assert_eq!(dependency_checker.operation_cache.len(), 2);
        let result = dependency_checker.process(operations[3].clone()).await;
        assert!(result.is_ok());
        assert_eq!(dependency_checker.operation_cache.len(), 3);
        let result = dependency_checker.process(operations[4].clone()).await;
        assert!(result.is_ok());
        assert_eq!(dependency_checker.operation_cache.len(), 4);

        let next = dependency_checker.next().await.unwrap();
        assert!(next.is_none());
    }
}
