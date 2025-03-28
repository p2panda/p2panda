// SPDX-License-Identifier: MIT OR Apache-2.0

use std::marker::PhantomData;

use p2panda_core::{Extensions, Hash, Operation};
use p2panda_store::{LogStore, OperationStore};
use thiserror::Error;

use crate::ordering::partial::{
    PartialOrder as InnerPartialOrder, PartialOrderError, PartialOrderStore,
};

/// Struct for processing p2panda operations into a partial order based on dependencies expressed
/// in their `previous` field.
///
/// This struct is a thin wrapper around ordering::PartialOrder struct which takes care of sorting
/// the operation dependency graph into a partial order. Here we have the addition of a `LogStore`
/// and `OperationStore` implementation (traits from `p2panda-store`).
#[derive(Debug)]
pub struct PartialOrder<L, E, OS, POS> {
    /// A store containing p2panda operations.
    ///
    /// It is assumed that any operations being processed by the PartialOrder struct are already
    /// present in the store, as we may need to retrieve them later.
    operation_store: OS,

    /// The inner PartialOrder struct which sorts the operation DAG into a partial order.
    inner: InnerPartialOrder<Hash, POS>,

    _phantom: PhantomData<(L, E)>,
}

impl<L, E, OS, POS> PartialOrder<L, E, OS, POS>
where
    OS: OperationStore<L, E> + LogStore<L, E>,
    POS: PartialOrderStore<Hash>,
    E: Extensions,
{
    /// Construct a new dependency checker from an operation store and dependency store.
    pub fn new(operation_store: OS, partial_order_store: POS) -> Self {
        let inner_dependency_checker = InnerPartialOrder::new(partial_order_store);
        PartialOrder {
            operation_store,
            inner: inner_dependency_checker,
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
        self.inner.process(hash, &previous).await?;
        Ok(())
    }

    /// Take the next ready operation from the queue.
    pub async fn next(&mut self) -> Result<Option<Operation<E>>, OperationDependencyCheckerError> {
        let Some(hash) = self.inner.next().await? else {
            return Ok(None);
        };

        if let Some((header, body)) = self
            .operation_store
            .get_operation(hash)
            .await
            .map_err(|err| OperationDependencyCheckerError::StoreError(err.to_string()))?
        {
            let operation = Operation { hash, header, body };
            Ok(Some(operation))
        } else {
            Err(OperationDependencyCheckerError::MissingOperation(hash))
        }
    }
}

#[derive(Debug, Error)]
pub enum OperationDependencyCheckerError {
    #[error(transparent)]
    CheckerError(#[from] PartialOrderError),

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

    use crate::ordering::partial::MemoryStore as PartialOrderMemoryStore;

    use super::PartialOrder;

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
        let partial_order_store = PartialOrderMemoryStore::default();
        let mut dependency_checker =
            PartialOrder::new(operation_store.clone(), partial_order_store);

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

        let result = dependency_checker.process(operations[3].clone()).await;
        assert!(result.is_ok());

        let result = dependency_checker.process(operations[2].clone()).await;
        assert!(result.is_ok());

        let result = dependency_checker.process(operations[1].clone()).await;
        assert!(result.is_ok());

        let result = dependency_checker.process(operations[0].clone()).await;
        assert!(result.is_ok());

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
    async fn missing_dependency() {
        let private_key = PrivateKey::new();
        let mut operation_store = MemoryStore::<u64>::default();
        let partial_order_store = PartialOrderMemoryStore::default();
        let mut dependency_checker =
            PartialOrder::new(operation_store.clone(), partial_order_store);

        let operations = setup(&private_key, &mut operation_store).await;

        let result = dependency_checker.process(operations[1].clone()).await;
        assert!(result.is_ok());
        let result = dependency_checker.process(operations[2].clone()).await;
        assert!(result.is_ok());
        let result = dependency_checker.process(operations[3].clone()).await;
        assert!(result.is_ok());
        let result = dependency_checker.process(operations[4].clone()).await;
        assert!(result.is_ok());

        let next = dependency_checker.next().await.unwrap();
        assert!(next.is_none());
    }
}
