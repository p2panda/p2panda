// SPDX-License-Identifier: MIT OR Apache-2.0

use std::marker::PhantomData;

use p2panda_core::traits::{Digest, OperationId};
// @TODO: Change these to p2panda_store when ready.
use p2panda_store_next::operations::OperationStore;
use p2panda_store_next::orderer::OrdererStore;
use thiserror::Error;
use tokio::sync::{Mutex, Notify};

use crate::orderer::{CausalOrderer, Ordering};
use crate::processors::Processor;

pub struct Orderer<T, ID, S> {
    inner: Mutex<CausalOrderer<ID, S>>,
    store: S,
    notify: Notify,
    _marker: PhantomData<T>,
}

impl<T, ID, S> Orderer<T, ID, S>
where
    ID: OperationId,
    S: Clone + OrdererStore<ID> + OperationStore<T, ID>,
{
    pub fn new(store: S) -> Self {
        let inner = CausalOrderer::new(store.clone());

        Self {
            inner: Mutex::new(inner),
            store,
            notify: Notify::new(),
            _marker: PhantomData,
        }
    }
}

impl<T, ID, S> Processor<T> for Orderer<T, ID, S>
where
    T: Digest<ID> + Ordering<ID>,
    ID: OperationId,
    S: OrdererStore<ID> + OperationStore<T, ID>,
{
    type Output = T;

    type Error = OrdererError<T, ID, S>;

    async fn process(&self, input: T) -> Result<(), Self::Error> {
        let mut inner = self.inner.lock().await;
        inner
            .process(input.hash(), input.dependencies())
            .await
            .map_err(|err| OrdererError::OrdererStore(err))?;

        self.notify.notify_one(); // Wake up any pending next call
        Ok(())
    }

    async fn next(&self) -> Result<Self::Output, Self::Error> {
        loop {
            let mut inner = self.inner.lock().await;

            if let Some(id) = inner.next().await.map_err(OrdererError::OrdererStore)? {
                return match self
                    .store
                    .get_operation(&id)
                    .await
                    .map_err(OrdererError::OperationStore)
                {
                    Ok(Some(operation)) => Ok(operation),
                    Ok(None) => Err(OrdererError::StoreInconsistency(id)),
                    Err(err) => Err(err),
                };
            }

            self.notify.notified().await;
        }
    }
}

#[derive(Debug, Error)]
pub enum OrdererError<T, ID, S>
where
    T: Ordering<ID>,
    ID: OperationId,
    S: OrdererStore<ID> + OperationStore<T, ID>,
{
    #[error("could not find item with id {0} in operation store")]
    StoreInconsistency(ID),

    #[error("{0}")]
    OrdererStore(<S as OrdererStore<ID>>::Error),

    #[error("{0}")]
    OperationStore(<S as OperationStore<T, ID>>::Error),
}

#[cfg(test)]
mod tests {
    use futures_util::stream;
    use p2panda_core::{Body, Hash, Header, Operation, PrivateKey};
    use p2panda_store_next::{memory::MemoryStore, operations::OperationStore};
    use serde::{Deserialize, Serialize};
    use tokio::task;
    use tokio_stream::StreamExt;

    use crate::StreamLayerExt;

    use super::{Orderer, Ordering};

    #[derive(Clone, Debug, Default, Serialize, Deserialize)]
    struct TestExtension {
        dependencies: Vec<Hash>,
    }

    impl Ordering<Hash> for Operation<TestExtension> {
        fn dependencies(&self) -> &[Hash] {
            &self.header.extensions.dependencies
        }
    }

    #[tokio::test]
    async fn out_of_order() {
        // Create two operations, one by Panda and one by Icebear. Panda's operation points at
        // Icebear's.
        let operation_panda = {
            let private_key = PrivateKey::new();
            let public_key = private_key.public_key();

            let body: Body = b"Hi, Icebear".to_vec().into();

            let mut header = Header {
                public_key,
                payload_size: body.size(),
                payload_hash: Some(body.hash()),
                extensions: TestExtension {
                    dependencies: vec![],
                },
                ..Default::default()
            };
            header.sign(&private_key);

            Operation {
                hash: header.hash(),
                header,
                body: Some(body),
            }
        };

        let operation_icebear = {
            let private_key = PrivateKey::new();
            let public_key = private_key.public_key();

            let body: Body = b"Hello, Pandasan!".to_vec().into();

            let mut header = Header {
                public_key,
                payload_size: body.size(),
                payload_hash: Some(body.hash()),
                extensions: TestExtension {
                    dependencies: vec![operation_panda.hash],
                },
                ..Default::default()
            };
            header.sign(&private_key);

            Operation {
                hash: header.hash(),
                header,
                body: Some(body),
            }
        };

        let local = task::LocalSet::new();

        local
            .run_until(async move {
                let store = MemoryStore::<Operation<TestExtension>, Hash>::new();

                // Insert operations into store.
                store
                    .insert_operation(&operation_panda.hash, operation_panda.clone())
                    .await
                    .unwrap();
                store
                    .insert_operation(&operation_icebear.hash, operation_icebear.clone())
                    .await
                    .unwrap();

                // Prepare processing pipeline for message ordering.
                let orderer = Orderer::new(store);

                let mut stream = stream::iter(vec![
                    // Process Icebear's operation first. It will arrive "out of order".
                    operation_icebear.clone(),
                    // Process Pandas's operation next. It will "free" Icebear's operation.
                    operation_panda.clone(),
                ])
                .layer(orderer);

                let operation = stream.next().await.unwrap().unwrap();
                assert_eq!(operation, operation_panda);

                let operation = stream.next().await.unwrap().unwrap();
                assert_eq!(operation, operation_icebear);
            })
            .await;
    }
}
