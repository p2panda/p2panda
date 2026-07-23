// SPDX-License-Identifier: MIT OR Apache-2.0

use std::borrow::Borrow;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::marker::PhantomData;

use p2panda_core::traits::Digest;
use p2panda_core::{Extensions, Hash, Operation};
use p2panda_store::Transaction;
use p2panda_store::operations::OperationStore;
use p2panda_store::orderer::OrdererStore;
use p2panda_store::processor::ProcessorStore;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::{Mutex, Notify};

use crate::Processor;
use crate::orderer::CausalOrderer;

pub trait OrdererMetadata<E> {
    /// Metadata attached to an input event we want to persist in database next to ordering info.
    type Metadata: Serialize + for<'de> Deserialize<'de>;

    /// Extract metadata from input.
    fn metadata(&self) -> Self::Metadata;

    /// Re-construct input from operation and persisted metadata.
    fn from_operation(operation: Operation<E>, meta: Self::Metadata) -> Self;
}

#[derive(Clone, Default, Debug, PartialEq, Eq)]
pub enum OrdererArgs {
    Process {
        dependencies: Vec<Hash>,
    },
    #[default]
    Ignore,
}

#[derive(Clone, Debug)]
pub enum OrdererResult {
    /// Item has unmet dependencies, is buffered and now in "pending" state.
    Pending,

    /// Item is "ready" and was a dependency for one or more operations which are "freed" now.
    ///
    /// This input may trigger zero to many ReadyOutput items to follow.
    ReadyInput { dependent_operations: Vec<Hash> },

    /// Item was buffered by orderer and is now marked as "ready" by another "input" item, to be
    /// finally forwarded in correct order.
    ReadyOutput,

    /// Item was ignored as specified in input arguments.
    Ignored,
}

impl OrdererResult {
    pub fn is_pending(&self) -> bool {
        matches!(self, OrdererResult::Pending)
    }
}

pub struct Orderer<S, T, E> {
    inner: Mutex<CausalOrderer<Hash, S>>,
    store: S,
    notify: Notify,
    // We're keeping an unbound, in-memory buffer of all "freed" events here. This means that if an
    // event frees n events, we will need to keep all n of them here. This shouldn't be a problem as
    // long as n stays low (<100 items).
    //
    // The alternative is to move the logic polling from the inner orderer into "next" which allows
    // a more atomic and memory-efficient streaming design where we only look at one item at a time.
    // We still need a way to tell the pipeline how many items got "freed" by it to allow the
    // correct input stream ordering to take place (see docs in pipeline.rs of p2panda crate), this
    // functionality should be possible to add to the inner orderer.
    queue: RefCell<VecDeque<(T, OrdererResult)>>,
    _marker: PhantomData<E>,
}

impl<S, T, E> Orderer<S, T, E>
where
    S: Clone + Transaction + OrdererStore<Hash> + OperationStore<Operation<E>, Hash>,
{
    pub fn new(store: S) -> Self {
        let inner = CausalOrderer::new(store.clone());

        Self {
            inner: Mutex::new(inner),
            store,
            notify: Notify::new(),
            queue: RefCell::new(VecDeque::new()),
            _marker: PhantomData,
        }
    }
}

impl<S, T, E> Processor<T> for Orderer<S, T, E>
where
    S: Transaction
        + OrdererStore<Hash>
        + OperationStore<Operation<E>, Hash>
        + ProcessorStore<T::Metadata>,
    T: OrdererMetadata<E> + Borrow<OrdererArgs> + Digest<Hash>,
    E: Extensions,
{
    type Output = (T, OrdererResult);

    type Error = (T, OrdererError);

    async fn process(&self, input: T) -> Result<(), Self::Error> {
        let args = input.borrow();

        if let OrdererArgs::Process { dependencies } = args {
            let inner = self.inner.lock().await;

            let permit = match self.store.begin().await {
                Ok(permit) => permit,
                Err(err) => return Err((input, OrdererError::Transaction(err.to_string()))),
            };

            match inner.process(input.hash(), dependencies).await {
                // a) Item has all dependencies met, we can directly mark it as "ready".
                Ok(true) => {
                    let mut dependent_operations = Vec::new();

                    loop {
                        match inner.next().await {
                            Ok(Some(id)) => {
                                // Ignore our own input.
                                if id != input.hash() {
                                    dependent_operations.push(id);
                                }

                                continue;
                            }
                            Ok(None) => {
                                break;
                            }
                            Err(err) => {
                                return Err((input, OrdererError::OrdererStore(err.to_string())));
                            }
                        }
                    }

                    if let Err(err) = self.store.commit(permit).await {
                        return Err((input, OrdererError::Transaction(err.to_string())));
                    }

                    let mut to_queue = Vec::new();

                    for id in &dependent_operations {
                        let operation = match self
                            .store
                            .get_operation(id)
                            .await
                            .map_err(|err| OrdererError::OperationStore(err.to_string()))
                        {
                            Ok(Some(operation)) => operation,
                            Ok(None) => {
                                return Err((input, OrdererError::StoreInconsistency(*id)));
                            }
                            Err(err) => return Err((input, err)),
                        };

                        let metadata = match self.store.get_event(id).await {
                            Ok(Some(metadata)) => metadata,
                            Ok(None) => {
                                return Err((input, OrdererError::StoreInconsistency(*id)));
                            }
                            Err(err) => {
                                return Err((input, OrdererError::ProcessorStore(err.to_string())));
                            }
                        };

                        to_queue.push((
                            T::from_operation(operation, metadata),
                            OrdererResult::ReadyOutput,
                        ));
                    }

                    // Always forward the current input first.
                    self.queue.borrow_mut().push_back((
                        input,
                        OrdererResult::ReadyInput {
                            dependent_operations,
                        },
                    ));

                    // .. followed by all items which have been marked as "ready" by input.
                    for item in to_queue {
                        self.queue.borrow_mut().push_back(item);
                    }
                }

                // b) Item doesn't have dependencies met yet, mark it as "pending", it is buffered now.
                Ok(false) => {
                    if let Err(err) = self.store.set_event(&input.hash(), &input.metadata()).await {
                        return Err((input, OrdererError::ProcessorStore(err.to_string())));
                    };

                    if let Err(err) = self.store.commit(permit).await {
                        return Err((input, OrdererError::Transaction(err.to_string())));
                    }

                    self.queue
                        .borrow_mut()
                        .push_back((input, OrdererResult::Pending));
                }
                Err(err) => return Err((input, OrdererError::OrdererStore(err.to_string()))),
            }
        } else {
            self.queue
                .borrow_mut()
                .push_back((input, OrdererResult::Ignored));
        }

        self.notify.notify_one(); // Wake up any pending next call

        Ok(())
    }

    async fn next(&self) -> Result<Self::Output, Self::Error> {
        // TODO: If we decide to handle all logic in the "process" part (less memory efficient
        // approach, see comment above) we should consider replacing the Processor trait with a
        // Streamas "next" is not required anymore.
        loop {
            if let Some(output) = self.queue.borrow_mut().pop_front() {
                return Ok(output);
            }

            self.notify.notified().await;
        }
    }
}

#[derive(Clone, Debug, Error)]
pub enum OrdererError {
    #[error("could not find item with id {0} in operation store")]
    StoreInconsistency(Hash),

    #[error("{0}")]
    OrdererStore(String),

    #[error("{0}")]
    OperationStore(String),

    #[error("{0}")]
    ProcessorStore(String),

    #[error("{0}")]
    Transaction(String),
}

#[cfg(test)]
mod tests {
    use std::assert_matches;
    use std::borrow::Borrow;

    use futures_util::stream;
    use p2panda_core::traits::Digest;
    use p2panda_core::{Body, Hash, Header, Operation, SigningKey, Topic};
    use p2panda_store::operations::OperationStore;
    use p2panda_store::{SqliteStore, tx_unwrap};
    use serde::{Deserialize, Serialize};
    use tokio::task;
    use tokio_stream::StreamExt;

    use crate::StreamLayerExt;
    use crate::orderer::OrdererMetadata;

    use super::{Orderer, OrdererArgs, OrdererResult};

    #[derive(Clone, Debug, Default, Serialize, Deserialize)]
    struct TestExtension {
        dependencies: Vec<Hash>,
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct Event {
        orderer_args: OrdererArgs,
        operation: Operation<TestExtension>,
    }

    impl Borrow<OrdererArgs> for Event {
        fn borrow(&self) -> &OrdererArgs {
            &self.orderer_args
        }
    }

    impl Digest<Hash> for Event {
        fn hash(&self) -> Hash {
            self.operation.hash
        }
    }

    #[derive(Clone, Debug, Serialize, Deserialize)]
    struct EventMetadata;

    impl OrdererMetadata<TestExtension> for Event {
        type Metadata = EventMetadata;

        fn metadata(&self) -> Self::Metadata {
            EventMetadata
        }

        // The `meta` parameter is a requirement of the trait function signature but we do not
        // require it for this test implementation, hence the lint bypass.
        #[allow(unused_variables)]
        fn from_operation(operation: Operation<TestExtension>, meta: Self::Metadata) -> Self {
            Self {
                orderer_args: OrdererArgs::Process {
                    dependencies: operation.header.extensions.dependencies.clone(),
                },
                operation,
            }
        }
    }

    #[tokio::test]
    async fn out_of_order() {
        // Create two operations, one by Panda and one by Icebear. Panda's operation points at
        // Icebear's.
        let operation_panda = {
            let signing_key = SigningKey::generate();
            let verifying_key = signing_key.verifying_key();

            let body: Body = b"Hi, Icebear".to_vec().into();

            let mut header = Header {
                verifying_key,
                payload_size: body.size(),
                payload_hash: Some(body.hash()),
                extensions: TestExtension {
                    dependencies: vec![],
                },
                ..Default::default()
            };
            header.sign(&signing_key);

            Operation {
                hash: header.hash(),
                header,
                body: Some(body),
            }
        };

        let event_panda = Event::from_operation(operation_panda.clone(), EventMetadata);

        let operation_icebear = {
            let signing_key = SigningKey::generate();
            let verifying_key = signing_key.verifying_key();

            let body: Body = b"Hello, Pandasan!".to_vec().into();

            let mut header = Header {
                verifying_key,
                payload_size: body.size(),
                payload_hash: Some(body.hash()),
                extensions: TestExtension {
                    dependencies: vec![operation_panda.hash],
                },
                ..Default::default()
            };
            header.sign(&signing_key);

            Operation {
                hash: header.hash(),
                header,
                body: Some(body),
            }
        };

        let event_icebear = Event::from_operation(operation_icebear.clone(), EventMetadata);

        let local = task::LocalSet::new();

        local
            .run_until(async move {
                let store = SqliteStore::temporary().await;

                // Insert operations into store.
                tx_unwrap!(store, {
                    let log_id = Topic::random();

                    store
                        .insert_operation(&operation_panda.hash, &operation_panda, &log_id)
                        .await
                        .unwrap();
                    store
                        .insert_operation(&operation_icebear.hash, &operation_icebear, &log_id)
                        .await
                        .unwrap();
                });

                // Prepare processing pipeline for message ordering.
                let orderer = Orderer::new(store);

                let mut stream = stream::iter(vec![
                    // Process Icebear's operation first. It will arrive "out of order".
                    event_icebear.clone(),
                    // Process Pandas's operation next. It will "free" Icebear's operation.
                    event_panda.clone(),
                ])
                .layer(orderer);

                // Icebear's event has a dependency on Panda's event so it remains in a pending
                // state for now.
                let (event, result) = stream.next().await.unwrap().unwrap();
                assert!(result.is_pending());
                assert_eq!(event, event_icebear);

                // Panda's event is released. It frees up Icebear's event which is now ready.
                let (event, result) = stream.next().await.unwrap().unwrap();
                assert_matches!(
                    result,
                    OrdererResult::ReadyInput {
                        dependent_operations: _
                    }
                );
                assert_eq!(event, event_panda);

                // Icebear's event has it's dependencies met and is released.
                let (event, result) = stream.next().await.unwrap().unwrap();
                assert_matches!(result, OrdererResult::ReadyOutput);
                assert_eq!(event, event_icebear);
            })
            .await;
    }
}
