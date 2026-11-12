// SPDX-License-Identifier: MIT OR Apache-2.0

use std::borrow::Borrow;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::fmt::{Display, Formatter};
use std::hash::Hash as StdHash;
use std::marker::PhantomData;
use std::str::FromStr;

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

#[derive(Copy, Clone, Debug, PartialOrd, Ord, PartialEq, Eq, StdHash)]
pub enum OrdererItem<ID> {
    Operation(Hash),
    Other(ID),
}

const SEPARATOR: char = '/';

impl<ID> Display for OrdererItem<ID>
where
    ID: Display,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Operation(hash) => write!(f, "operation{SEPARATOR}{}", hash),
            Self::Other(value) => write!(f, "other{SEPARATOR}{}", value),
        }
    }
}

impl<ID> From<OrdererItem<ID>> for Hash
where
    ID: Into<Hash>,
{
    fn from(val: OrdererItem<ID>) -> Self {
        match val {
            OrdererItem::<ID>::Operation(operation_id) => operation_id,
            OrdererItem::<ID>::Other(value) => value.into(),
        }
    }
}

impl<ID> FromStr for OrdererItem<ID>
where
    ID: FromStr,
    ID::Err: Display,
{
    type Err = OrdererItemParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let mut parts = value.split(SEPARATOR);

        let Some(prefix) = parts.next() else {
            return Err(OrdererItemParseError("missing prefix".into()));
        };

        let Some(value) = parts.next() else {
            return Err(OrdererItemParseError("missing value".into()));
        };

        let result = match prefix {
            "operation" => Self::Operation(
                Hash::from_str(value).map_err(|err| OrdererItemParseError(err.to_string()))?,
            ),
            "other" => Self::Other(
                ID::from_str(value).map_err(|err| OrdererItemParseError(err.to_string()))?,
            ),
            _ => return Err(OrdererItemParseError("unknown variant".into())),
        };

        Ok(result)
    }
}

#[derive(Debug, Error)]
#[error("{0}")]
pub struct OrdererItemParseError(String);

pub trait OrdererMetadata<E> {
    /// Metadata attached to an input event we want to persist in database next to ordering info.
    type Metadata: Serialize + for<'de> Deserialize<'de>;

    /// Extract metadata from input.
    fn metadata(&self) -> Self::Metadata;

    /// Re-construct input from operation and persisted metadata.
    fn from_operation(operation: Operation<E>, meta: Self::Metadata) -> Self;
}

#[derive(Clone, Default, Debug, PartialEq, Eq)]
pub enum OrdererArgs<ID>
where
    ID: Clone + Display + FromStr,
{
    /// The orderer can manage multiple dependency trees for one single operation.
    ///
    /// An operation can be an important dependency in an "operation graph" but also in other
    /// graphs, for example in an "encryption secret" graph. To express this, users can set
    /// additional identifiers this item should represent for other graphs in the `other_ids` field.
    Process {
        /// Id of the operation coming in.
        operation_id: Hash,

        /// Optional, additional ids.
        other_ids: Vec<ID>,

        /// Declared dependencies from incoming item.
        dependencies: Vec<OrdererItem<ID>>,
    },
    #[default]
    Ignore,
}

#[derive(Clone, Debug)]
pub enum OrdererResult<ID> {
    /// Item has unmet dependencies, is buffered and now in "pending" state.
    Pending,

    /// Item is "ready" and was a dependency for one or more items which are "freed" now.
    ///
    /// This input may trigger zero to many ReadyOutput items to follow.
    ReadyInput {
        dependent_operations: Vec<Hash>,
        dependent_others: Vec<ID>,
    },

    /// Item was buffered by orderer and is now marked as "ready" by another "input" item, to be
    /// finally forwarded in correct order.
    ReadyOutput,

    /// Item was ignored as specified in input arguments.
    Ignored,
}

impl<ID> OrdererResult<ID> {
    pub fn is_pending(&self) -> bool {
        matches!(self, OrdererResult::Pending)
    }
}

pub struct Orderer<S, T, E, ID> {
    inner: Mutex<CausalOrderer<OrdererItem<ID>, S>>,
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
    queue: RefCell<VecDeque<(T, OrdererResult<ID>)>>,
    _marker: PhantomData<E>,
}

impl<S, T, E, ID> Orderer<S, T, E, ID>
where
    S: Clone + Transaction + OrdererStore<OrdererItem<ID>> + OperationStore<Operation<E>, Hash>,
    ID: Clone,
{
    pub fn new(store: S) -> Self {
        Self::from_namespace(store, "default")
    }

    pub fn from_namespace(store: S, namespace: impl AsRef<str>) -> Self {
        let inner = CausalOrderer::new(store.clone()).with_namespace(namespace);

        Self {
            inner: Mutex::new(inner),
            store,
            notify: Notify::new(),
            queue: RefCell::new(VecDeque::new()),
            _marker: PhantomData,
        }
    }
}

impl<S, T, E, ID> Processor<T> for Orderer<S, T, E, ID>
where
    S: Transaction
        + OrdererStore<OrdererItem<ID>>
        + OperationStore<Operation<E>, Hash>
        + ProcessorStore<T::Metadata>,
    T: OrdererMetadata<E> + Borrow<OrdererArgs<ID>>,
    E: Extensions,
    ID: Copy + Clone + PartialEq + Display + FromStr + Into<Hash>,
{
    type Output = (T, OrdererResult<ID>);

    type Error = (T, OrdererError);

    async fn process(&self, input: T) -> Result<(), Self::Error> {
        let args = input.borrow();

        if let OrdererArgs::Process {
            operation_id: operation,
            other_ids: others,
            dependencies,
        } = args
        {
            let inner = self.inner.lock().await;

            let permit = match self.store.begin().await {
                Ok(permit) => permit,
                Err(err) => return Err((input, OrdererError::Transaction(err.to_string()))),
            };

            let items: Vec<OrdererItem<ID>> = std::iter::once(OrdererItem::Operation(*operation))
                .chain(others.iter().map(|id| OrdererItem::Other(*id)))
                .collect();

            // All items share the same dependencies, so they are either all ready or all pending.
            let mut result = Ok(true);
            for item in &items {
                result = inner.process(*item, dependencies).await;
                if !matches!(result, Ok(true)) {
                    break;
                }
            }

            match result {
                // a) Item has all dependencies met, we can directly mark it as "ready".
                Ok(true) => {
                    let mut dependent_operations = Vec::new();
                    let mut dependent_others = Vec::new();

                    loop {
                        match inner.next().await {
                            Ok(Some(next)) => {
                                // Ignore our own input.
                                if items.contains(&next) {
                                    continue;
                                }

                                match next {
                                    OrdererItem::Operation(id) => {
                                        dependent_operations.push(id);
                                    }
                                    OrdererItem::Other(id) => {
                                        dependent_others.push(id);
                                    }
                                }
                            }
                            Ok(None) => {
                                break;
                            }
                            Err(err) => {
                                return Err((input, OrdererError::OrdererStore(err.to_string())));
                            }
                        }
                    }

                    let mut to_queue = Vec::new();

                    for id in &dependent_operations {
                        let operation = match self
                            .store
                            .get_operation_tx(id)
                            .await
                            .map_err(|err| OrdererError::OperationStore(err.to_string()))
                        {
                            Ok(Some(operation)) => operation,
                            Ok(None) => {
                                return Err((input, OrdererError::StoreInconsistency(*id)));
                            }
                            Err(err) => return Err((input, err)),
                        };

                        let metadata = match self.store.get_event_tx(&inner.namespace(), id).await {
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

                    if let Err(err) = self.store.commit(permit).await {
                        return Err((input, OrdererError::Transaction(err.to_string())));
                    }

                    // Always forward the current input first.
                    self.queue.borrow_mut().push_back((
                        input,
                        OrdererResult::ReadyInput {
                            dependent_operations,
                            dependent_others,
                        },
                    ));

                    // .. followed by all items which have been marked as "ready" by input.
                    for item in to_queue {
                        self.queue.borrow_mut().push_back(item);
                    }
                }

                // b) Item doesn't have dependencies met yet, mark it "pending", it is buffered now.
                Ok(false) => {
                    if let Err(err) = self
                        .store
                        .set_event_tx(&inner.namespace(), operation, &input.metadata())
                        .await
                    {
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
        // Stream as "next" is not required anymore.
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

    use crate::orderer::processor::OrdererItem;
    use crate::{Processor, StreamLayerExt};

    use super::{Orderer, OrdererArgs, OrdererMetadata, OrdererResult};

    #[derive(Clone, Debug, Default, Serialize, Deserialize)]
    struct TestExtension {
        dependencies: Vec<Hash>,
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct Event {
        orderer_args: OrdererArgs<Topic>,
        operation: Operation<TestExtension>,
    }

    impl Borrow<OrdererArgs<Topic>> for Event {
        fn borrow(&self) -> &OrdererArgs<Topic> {
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
                    operation_id: operation.hash(),
                    other_ids: vec![],
                    dependencies: operation
                        .header
                        .extensions
                        .dependencies
                        .iter()
                        .map(|id| OrdererItem::Operation(*id))
                        .collect(),
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
            let body: Body = b"Hi, Icebear".to_vec().into();

            let header = Header::builder().body(&body).build(
                &signing_key,
                TestExtension {
                    dependencies: vec![],
                },
            );

            Operation::from_parts(header, Some(body))
        };

        let event_panda = Event::from_operation(operation_panda.clone(), EventMetadata);

        let operation_icebear = {
            let signing_key = SigningKey::generate();
            let body: Body = b"Hello, Pandasan!".to_vec().into();

            let header = Header::builder().body(&body).build(
                &signing_key,
                TestExtension {
                    dependencies: vec![operation_panda.hash],
                },
            );

            Operation::from_parts(header, Some(body))
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
                        .insert_operation_tx(&operation_panda.hash, &operation_panda, &log_id)
                        .await
                        .unwrap();
                    store
                        .insert_operation_tx(&operation_icebear.hash, &operation_icebear, &log_id)
                        .await
                        .unwrap();
                });

                // Prepare processing pipeline for message ordering.
                let orderer_1 = Orderer::from_namespace(store.clone(), "first");

                let mut stream = stream::iter(vec![
                    // Process Icebear's operation first. It will arrive "out of order".
                    event_icebear.clone(),
                    // Process Pandas's operation next. It will "free" Icebear's operation.
                    event_panda.clone(),
                ])
                .layer(orderer_1);

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
                        dependent_operations: _,
                        dependent_others: _,
                    }
                );
                assert_eq!(event, event_panda);

                // Icebear's event has it's dependencies met and is released.
                let (event, result) = stream.next().await.unwrap().unwrap();
                assert_matches!(result, OrdererResult::ReadyOutput);
                assert_eq!(event, event_icebear);

                // Make sure another, namespaced orderer is not affected:
                let orderer_2 = Orderer::from_namespace(store, "second");
                orderer_2.process(event_icebear.clone()).await.unwrap();
                let (_event, result) = orderer_2.next().await.unwrap();
                assert_matches!(
                    result,
                    OrdererResult::Pending,
                    "panda's event was not processed yet for second orderer"
                );
            })
            .await;
    }

    #[tokio::test]
    async fn non_operation_dependencies() {
        let space_id = Topic::random();

        let operation_welcome = {
            let signing_key = SigningKey::generate();
            let body: Body = b"Nein, Nein, Nein".to_vec().into();

            let header = Header::builder().body(&body).build(
                &signing_key,
                TestExtension {
                    dependencies: vec![],
                },
            );

            Operation::from_parts(header, Some(body))
        };

        let event_welcome = Event {
            orderer_args: OrdererArgs::Process {
                operation_id: operation_welcome.hash,
                other_ids: vec![space_id],
                dependencies: vec![],
            },
            operation: operation_welcome.clone(),
        };

        let operation = {
            let signing_key = SigningKey::generate();
            let body: Body = b"Ja, Ja, Ja".to_vec().into();

            let header = Header::builder().body(&body).build(
                &signing_key,
                TestExtension {
                    dependencies: vec![],
                },
            );

            Operation::from_parts(header, Some(body))
        };

        let event_buffered = Event {
            orderer_args: OrdererArgs::Process {
                operation_id: operation.hash,
                other_ids: vec![],
                dependencies: vec![OrdererItem::Other(space_id)],
            },
            operation: operation.clone(),
        };

        let local = task::LocalSet::new();

        local
            .run_until(async move {
                // Prepare processing pipeline for message ordering.
                let orderer = {
                    let store = SqliteStore::temporary().await;

                    tx_unwrap!(store, {
                        let log_id = Topic::random();

                        store
                            .insert_operation_tx(&operation.hash, &operation, &log_id)
                            .await
                            .unwrap();
                        store
                            .insert_operation_tx(
                                &operation_welcome.hash,
                                &operation_welcome,
                                &log_id,
                            )
                            .await
                            .unwrap();
                    });

                    Orderer::from_namespace(store.clone(), "first")
                };

                let mut stream = stream::iter(vec![
                    // Process operation first. It will arrive "out of order".
                    event_buffered.clone(),
                    // Process "welcome" event next. It will "free" the operation.
                    event_welcome.clone(),
                ])
                .layer(orderer);

                let (event, result) = stream.next().await.unwrap().unwrap();
                assert!(result.is_pending());
                assert_eq!(event, event_buffered);

                let (event, result) = stream.next().await.unwrap().unwrap();
                assert_matches!(
                    result,
                    OrdererResult::ReadyInput {
                        dependent_operations: _,
                        dependent_others: _,
                    }
                );
                assert_eq!(event, event_welcome);

                let (event, result) = stream.next().await.unwrap().unwrap();
                assert_matches!(result, OrdererResult::ReadyOutput);
                assert_eq!(event.hash(), event_buffered.hash());
            })
            .await;
    }
}
