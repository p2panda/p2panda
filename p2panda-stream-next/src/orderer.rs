// SPDX-License-Identifier: MIT OR Apache-2.0

use std::cell::RefCell;
use std::fmt::Display;
use std::hash::Hash as StdHash;
use std::marker::PhantomData;

use p2panda_store_next::operations::OperationStore;
use p2panda_stream::orderer::{PartialOrder, PartialOrderStore};
use thiserror::Error;
use tokio::sync::Notify;

use crate::Processor;

// @TODO: Decide where this lives. Is it part of the core crate?
pub trait OperationId: Clone + Copy + PartialEq + Eq + Display + StdHash {}

pub trait Ordering<ID> {
    // @TODO: Is this part of `Ordering` or another "id" trait?
    fn id(&self) -> &ID;

    fn dependencies(&self) -> &[ID];
}

pub struct Orderer<T, ID, PS, OS> {
    inner: RefCell<PartialOrder<ID, PS>>,
    operation_store: OS,
    notify: Notify,
    _marker: PhantomData<T>,
}

impl<T, ID, PS, OS> Orderer<T, ID, PS, OS>
where
    ID: OperationId,
    PS: PartialOrderStore<ID>,
    OS: OperationStore<T, ID>,
{
    pub fn new(store: PS, operation_store: OS) -> Self {
        let inner = PartialOrder::new(store);

        Self {
            inner: RefCell::new(inner),
            operation_store,
            notify: Notify::new(),
            _marker: PhantomData,
        }
    }
}

impl<T, ID, PS, OS> Processor<T> for Orderer<T, ID, PS, OS>
where
    T: Ordering<ID>,
    ID: OperationId,
    PS: PartialOrderStore<ID>,
    OS: OperationStore<T, ID>,
{
    type Output = T;

    type Error = OrdererError<T, ID, PS, OS>;

    async fn process(&self, input: T) -> Result<(), Self::Error> {
        let mut inner = self.inner.borrow_mut();
        inner
            .process(*input.id(), input.dependencies())
            .await
            .map_err(|err| OrdererError::PartialOrderStore(err))?;
        self.notify.notify_one(); // Wake up any pending next call
        Ok(())
    }

    async fn next(&self) -> Result<Self::Output, Self::Error> {
        loop {
            let mut inner = self.inner.borrow_mut();
            match inner.next().await {
                Ok(Some(_id)) => {
                    // @TODO: Get item from database.
                    todo!()
                }
                Ok(None) => (),
                Err(err) => return Err(OrdererError::PartialOrderStore(err)),
            }

            self.notify.notified().await;
        }
    }
}

#[derive(Debug, Error)]
pub enum OrdererError<T, ID, PS, OS>
where
    T: Ordering<ID>,
    ID: OperationId,
    PS: PartialOrderStore<ID>,
    OS: OperationStore<T, ID>,
{
    #[error("{0}")]
    PartialOrderStore(PS::Error),

    #[error("{0}")]
    OperationStore(OS::Error),
}

#[cfg(test)]
mod tests {
    use futures_util::stream;
    use p2panda_core::{Body, Hash, Header, Operation, PrivateKey};
    use p2panda_stream::orderer::MemoryStore;
    use serde::{Deserialize, Serialize};
    use tokio::task;

    use crate::StreamLayerExt;

    use super::{OperationId, Orderer, Ordering};

    // @TODO: This should be implemented automatically in our crates.
    impl OperationId for Hash {}

    #[derive(Clone, Debug, Serialize, Deserialize)]
    struct TestExtension {
        dependencies: Vec<Hash>,
    }

    impl Ordering<Hash> for Operation<TestExtension> {
        fn id(&self) -> &Hash {
            &self.hash
        }

        fn dependencies(&self) -> &[Hash] {
            match self.header.extensions {
                Some(ref extensions) => &extensions.dependencies,
                None => &[],
            }
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
                extensions: Some(TestExtension {
                    dependencies: vec![],
                }),
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
                extensions: Some(TestExtension {
                    dependencies: vec![operation_panda.hash],
                }),
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
                // @TODO: Finish test.
                // let store = MemoryStore::default();
                //
                // // Prepare processing pipeline for message ordering.
                // let orderer = Orderer::new(store, operation_store);
                //
                // // Process Icebear's operation first. It will arrive "out of order".
                // // Process Pandas's operation next. It will "free" Icebear's operation.
                // let mut _stream =
                //     stream::iter(vec![operation_icebear, operation_panda]).layer(orderer);
            })
            .await;
    }
}
