// SPDX-License-Identifier: MIT OR Apache-2.0

use std::borrow::Borrow;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::fmt::Debug;
use std::marker::PhantomData;

use p2panda_auth::traits::{Conditions, Operation as GroupsOperationTrait};
use p2panda_auth::{GroupsExtensionArgs, group};
use p2panda_core::{Extension, Extensions, Hash, LogId, VerifyingKey};
use p2panda_store::groups::GroupsStore;
use p2panda_store::{SqliteError, SqliteStore, Transaction};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::Notify;
use tracing::{debug, trace};

use crate::Processor;
use crate::groups::{GroupsArgs, GroupsOperation};

type GroupsCrdt<C> = group::GroupCrdt<VerifyingKey, Hash, GroupsOperation<C>, C, StrongRemove<C>>;
type GroupsCrdtError<C> =
    group::GroupCrdtError<VerifyingKey, Hash, GroupsOperation<C>, C, StrongRemove<C>>;
type StrongRemove<C> = group::resolver::StrongRemove<VerifyingKey, Hash, GroupsOperation<C>, C>;
type GroupsState<C> = group::GroupCrdtState<VerifyingKey, Hash, GroupsOperation<C>, C>;

#[derive(Clone)]
pub enum GroupsResult {
    Processed,
    Noop,
}

impl GroupsResult {
    pub fn was_processed(self) -> bool {
        match self {
            Self::Processed => true,
            Self::Noop => false,
        }
    }
}

/// Processor for groups operations.
pub struct Groups<SID, T, E, L, C = ()> {
    store: SqliteStore,
    notify: Notify,
    queue: RefCell<VecDeque<(T, GroupsResult)>>,
    _marker: PhantomData<(SID, E, L, C)>,
}

impl<SID, T, E, L, C> Groups<SID, T, E, L, C>
where
    E: Extensions + Extension<GroupsExtensionArgs<C>> + Extension<L>,
    L: LogId,
    C: Conditions + Serialize + for<'a> Deserialize<'a>,
{
    pub fn new(store: SqliteStore) -> Self {
        Self {
            store: store.clone(),
            notify: Notify::new(),
            queue: RefCell::new(VecDeque::new()),
            _marker: Default::default(),
        }
    }
}

impl<SID, T, E, L, C> Processor<T> for Groups<SID, T, E, L, C>
where
    SID: for<'a> Deserialize<'a> + Serialize,
    T: Borrow<GroupsArgs<SID, E>>,
    E: Extensions + Extension<GroupsExtensionArgs<C>> + Extension<L>,
    L: LogId,
    C: Conditions + Serialize + for<'a> Deserialize<'a>,
{
    type Output = (T, GroupsResult);

    type Error = (T, GroupsError<C>);

    async fn process(&self, input: T) -> Result<(), Self::Error> {
        let input_args: &GroupsArgs<SID, E> = input.borrow();

        let result = if let GroupsArgs::Process {
            state_id,
            operation,
        } = input_args &&
            // Extract GroupArgs from the extension headers of an Operation<E>.
            //
            // If this returns None then the groups extension was not present and we consider this
            // a non-groups operation which does not require processing.
            let Some(args) = operation.header.extension::<GroupsExtensionArgs<C>>()
        {
            // Construct a GroupsOperation from an Operation<E>.
            let groups_operation = GroupsOperation {
                id: operation.hash,
                author: operation.header.verifying_key,
                dependencies: args.dependencies,
                group_id: args.group_id,
                action: args.action,
            };

            // Start a transaction for all following database actions.
            let permit = match self.store.begin().await {
                Ok(permit) => permit,
                Err(err) => return Err((input, err.into())),
            };

            // Retrieve the current groups state from the store.
            let mut y = match GroupsStore::<SID, GroupsState<C>>::get_groups_state_tx(
                &self.store,
                state_id,
            )
            .await
            {
                Err(err) => return Err((input, err.into())),
                Ok(Some(y)) => y,
                Ok(None) => Default::default(),
            };

            debug!(
                group_id = groups_operation.group_id().to_hex(),
                "current group membership: {:?}",
                y.members(groups_operation.group_id())
            );

            debug!(
                id = groups_operation.id.to_hex(),
                "apply operation to group state"
            );
            y = match GroupsCrdt::process(y, &groups_operation) {
                Ok(y) => y,
                Err(err) => return Err((input, err.into())),
            };

            // Set the groups state after processing is finished.
            if let Err(err) = self.store.set_groups_state_tx(state_id, &y).await {
                return Err((input, err.into()));
            }

            // Commit the open transaction.
            if let Err(err) = self.store.commit(permit).await {
                return Err((input, err.into()));
            }

            debug!(
                group_id = groups_operation.group_id().to_hex(),
                "new group membership: {:?}",
                y.members(groups_operation.group_id())
            );

            (input, GroupsResult::Processed)
        } else {
            trace!("ignore non-groups operation");
            (input, GroupsResult::Noop)
        };

        self.queue.borrow_mut().push_back(result);
        self.notify.notify_one(); // Wake up any pending recv.

        Ok(())
    }

    async fn next(&self) -> Result<Self::Output, Self::Error> {
        loop {
            if let Some(item) = self.queue.borrow_mut().pop_front() {
                return Ok(item);
            }

            // Wait for notification that an item was added.
            self.notify.notified().await;
        }
    }
}

/// Error types which can occur in the groups processor.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Error)]
pub enum GroupsError<C>
where
    C: Conditions,
{
    #[error(transparent)]
    Store(#[from] SqliteError),

    #[error(transparent)]
    Groups(#[from] GroupsCrdtError<C>),

    #[error("missing operation: {0}")]
    MissingOperation(Hash),

    #[error("operation retrieved from store missing groups arguments: {0}")]
    MissingGroupsArgs(Hash),

    #[error("missing \"log id\" operation extension")]
    MissingLogId,
}

#[cfg(test)]
mod tests {
    use std::borrow::Borrow;

    use p2panda_auth::group::{GroupAction, GroupCrdtState, GroupMember};
    use p2panda_auth::{Access, GroupsExtensionArgs};
    use p2panda_core::test_utils::{TestLog, setup_logging};
    use p2panda_core::traits::Digest;
    use p2panda_core::{Extension, Hash, Header, Operation, SigningKey, Topic, VerifyingKey};
    use p2panda_store::groups::GroupsStore;
    use p2panda_store::{SqliteStore, Transaction};
    use serde::{Deserialize, Serialize};

    use crate::Processor;
    use crate::groups::GroupsOperation;
    use crate::ingest::{Ingest, IngestArgs};
    use crate::orderer::{Orderer, Ordering};

    use super::GroupsArgs;

    type LogId = usize;
    type StateId = u8;
    type GroupsState = GroupCrdtState<VerifyingKey, Hash, GroupsOperation, ()>;
    type Groups =
        crate::groups::Groups<StateId, GroupsArgs<StateId, TestExtensions>, TestExtensions, LogId>;

    const LOG_ID: usize = 0;

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
    struct TestExtensions {
        log_id: LogId,
        dependencies: Vec<Hash>,
        groups: Option<GroupsExtensionArgs>,
    }

    impl Extension<LogId> for TestExtensions {
        fn extract(header: &Header<Self>) -> Option<LogId> {
            Some(header.extensions.log_id)
        }
    }

    impl Extension<GroupsExtensionArgs> for TestExtensions {
        fn extract(header: &Header<Self>) -> Option<GroupsExtensionArgs> {
            header.extensions.groups.clone()
        }
    }

    impl Ordering<Hash> for Operation<TestExtensions> {
        fn dependencies(&self) -> &[Hash] {
            &self.header.extensions.dependencies
        }
    }

    impl From<GroupsExtensionArgs> for TestExtensions {
        fn from(args: GroupsExtensionArgs) -> Self {
            TestExtensions {
                log_id: LOG_ID,
                dependencies: args.dependencies.clone(),
                groups: Some(args),
            }
        }
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct IngestEvent<E> {
        pub operation: Operation<E>,
        pub args: IngestArgs<usize, Topic>,
    }

    impl<E> Borrow<IngestArgs<usize, Topic>> for IngestEvent<E> {
        fn borrow(&self) -> &IngestArgs<usize, Topic> {
            &self.args
        }
    }

    impl<E> Borrow<Operation<E>> for IngestEvent<E> {
        fn borrow(&self) -> &Operation<E> {
            &self.operation
        }
    }

    #[tokio::test]
    async fn basic_processing() {
        setup_logging();

        let topic = Topic::random();

        let state_id = 0;
        let group_id = SigningKey::generate().verifying_key();

        let alice_log = TestLog::new();
        let bobby_log = TestLog::new();

        let alice = alice_log.author();
        let bobby = bobby_log.author();

        let args = GroupsExtensionArgs {
            group_id,
            action: GroupAction::Create {
                initial_members: vec![
                    (GroupMember::Individual(alice), <Access>::manage()),
                    (GroupMember::Individual(bobby), <Access>::manage()),
                ],
            },
            dependencies: vec![],
        };
        let op_00: Operation<TestExtensions> = alice_log.operation(&[], TestExtensions::from(args));

        let store = SqliteStore::temporary().await;

        // The operation needs to be ingested before it can be processed by the groups processor.
        let ingest = Ingest::new(store.clone());
        ingest
            .process(IngestEvent {
                operation: op_00.clone(),
                args: IngestArgs {
                    log_id: LOG_ID.into(),
                    topic,
                    prune_flag: false,
                },
            })
            .await
            .unwrap();

        let groups = Groups::new(store.clone());
        groups
            .process(GroupsArgs::Process {
                state_id,
                operation: op_00,
            })
            .await
            .unwrap();
        let (_processed_op, result) = groups.next().await.unwrap();
        assert!(result.was_processed());

        let permit = store.begin().await.unwrap();
        let y: GroupsState = store.get_groups_state_tx(&state_id).await.unwrap().unwrap();
        store.commit(permit).await.unwrap();

        let members = y.members(group_id);
        assert_eq!(members.len(), 2);
        assert!(members.contains(&(alice, Access::manage())));
        assert!(members.contains(&(bobby, Access::manage())));
    }

    #[tokio::test]
    async fn ooo_operations() {
        setup_logging();
        let topic = Topic::random();

        let state_id = 0;
        let group_id = SigningKey::generate().verifying_key();

        let alice_log = TestLog::new();
        let bobby_log = TestLog::new();
        let cathy_log = TestLog::new();

        let alice = alice_log.author();
        let bobby = bobby_log.author();
        let cathy = cathy_log.author();

        // Alice operation.
        let args = GroupsExtensionArgs {
            group_id,
            action: GroupAction::Create {
                initial_members: vec![
                    (GroupMember::Individual(alice), <Access>::manage()),
                    (GroupMember::Individual(bobby), <Access>::manage()),
                ],
            },
            dependencies: vec![],
        };
        let op_00: Operation<TestExtensions> = alice_log.operation(&[], TestExtensions::from(args));

        // Bobby operation.
        let args = GroupsExtensionArgs {
            group_id,
            action: GroupAction::Add {
                member: GroupMember::Individual(cathy),
                access: Access::manage(),
            },
            dependencies: vec![op_00.hash()],
        };
        let op_01: Operation<TestExtensions> = bobby_log.operation(&[], TestExtensions::from(args));

        // Cathy operation.
        let args = GroupsExtensionArgs {
            group_id,
            action: GroupAction::Remove {
                member: GroupMember::Individual(alice),
            },
            dependencies: vec![op_01.hash()],
        };
        let op_02: Operation<TestExtensions> = cathy_log.operation(&[], TestExtensions::from(args));

        let store = SqliteStore::temporary().await;
        let ingest = Ingest::new(store.clone());
        let orderer = Orderer::new(store.clone());
        let groups = Groups::new(store.clone());

        ingest
            .process(IngestEvent {
                operation: op_02.clone(),
                args: IngestArgs {
                    log_id: LOG_ID.into(),
                    topic,
                    prune_flag: false,
                },
            })
            .await
            .unwrap();

        ingest
            .process(IngestEvent {
                operation: op_01.clone(),
                args: IngestArgs {
                    log_id: LOG_ID.into(),
                    topic,
                    prune_flag: false,
                },
            })
            .await
            .unwrap();

        ingest
            .process(IngestEvent {
                operation: op_00.clone(),
                args: IngestArgs {
                    log_id: LOG_ID.into(),
                    topic,
                    prune_flag: false,
                },
            })
            .await
            .unwrap();

        // Operations are processed by the orderer in reverse order.
        orderer.process(op_02).await.unwrap();
        orderer.process(op_01).await.unwrap();
        orderer.process(op_00).await.unwrap();

        // Each operation freed from the orderer is processed by the groups processor.
        for _op in 0..3 {
            let next_op = orderer.next().await.unwrap();
            groups
                .process(GroupsArgs::Process {
                    state_id,
                    operation: next_op,
                })
                .await
                .unwrap()
        }

        let permit = store.begin().await.unwrap();
        let y: GroupsState = store.get_groups_state_tx(&state_id).await.unwrap().unwrap();
        store.commit(permit).await.unwrap();

        let members = y.members(group_id);
        assert_eq!(members.len(), 2);
        assert!(members.contains(&(bobby, Access::manage())));
        assert!(members.contains(&(cathy, Access::manage())));
        assert!(!members.contains(&(alice, Access::manage())));
    }

    #[tokio::test]
    async fn device_groups_single_context() {
        let topic = Topic::random();

        // All operations are processed on the same groups state context.
        let state_id = 1;

        let alice_log = TestLog::new();
        let bobby_log = TestLog::new();
        let cathy_log = TestLog::new();

        let alice = alice_log.author();
        let bobby = bobby_log.author();
        let cathy = cathy_log.author();

        let alice_store = SqliteStore::temporary().await;
        let bobby_store = SqliteStore::temporary().await;
        let cathy_store = SqliteStore::temporary().await;

        let alice_device_group = SigningKey::generate().verifying_key();
        let bobby_device_group = SigningKey::generate().verifying_key();
        let cathy_device_group = SigningKey::generate().verifying_key();

        let ab_chat = SigningKey::generate().verifying_key();
        let bc_chat = SigningKey::generate().verifying_key();

        let alice_groups = Groups::new(alice_store.clone());
        let bobby_groups = Groups::new(bobby_store.clone());
        let cathy_groups = Groups::new(cathy_store.clone());

        // All members create their own device groups and process them on their own stores.

        let args = GroupsExtensionArgs {
            group_id: alice_device_group,
            action: GroupAction::Create {
                initial_members: vec![(GroupMember::Individual(alice), Access::manage())],
            },
            dependencies: vec![],
        };
        let create_alice_device_00: Operation<TestExtensions> =
            alice_log.operation(&[], TestExtensions::from(args));

        let alice_ingest = Ingest::new(alice_store.clone());
        alice_ingest
            .process(IngestEvent {
                operation: create_alice_device_00.clone(),
                args: IngestArgs {
                    log_id: LOG_ID.into(),
                    topic,
                    prune_flag: false,
                },
            })
            .await
            .unwrap();

        alice_groups
            .process(GroupsArgs::Process {
                state_id,
                operation: create_alice_device_00.clone(),
            })
            .await
            .unwrap();

        let args = GroupsExtensionArgs {
            group_id: bobby_device_group,
            action: GroupAction::Create {
                initial_members: vec![(GroupMember::Individual(bobby), Access::manage())],
            },
            dependencies: vec![],
        };
        let create_bobby_device_01: Operation<TestExtensions> =
            bobby_log.operation(&[], TestExtensions::from(args));

        let bobby_ingest = Ingest::new(bobby_store.clone());
        bobby_ingest
            .process(IngestEvent {
                operation: create_bobby_device_01.clone(),
                args: IngestArgs {
                    log_id: LOG_ID.into(),
                    topic,
                    prune_flag: false,
                },
            })
            .await
            .unwrap();

        bobby_groups
            .process(GroupsArgs::Process {
                state_id,
                operation: create_bobby_device_01.clone(),
            })
            .await
            .unwrap();

        let args = GroupsExtensionArgs {
            group_id: cathy_device_group,
            action: GroupAction::Create {
                initial_members: vec![(GroupMember::Individual(cathy), Access::manage())],
            },
            dependencies: vec![],
        };
        let create_cathy_device_02: Operation<TestExtensions> =
            cathy_log.operation(&[], TestExtensions::from(args));

        let cathy_ingest = Ingest::new(cathy_store.clone());
        cathy_ingest
            .process(IngestEvent {
                operation: create_cathy_device_02.clone(),
                args: IngestArgs {
                    log_id: LOG_ID.into(),
                    topic,
                    prune_flag: false,
                },
            })
            .await
            .unwrap();

        cathy_groups
            .process(GroupsArgs::Process {
                state_id,
                operation: create_cathy_device_02.clone(),
            })
            .await
            .unwrap();

        // Alice creates chat with Bobby.
        //
        // First they process "create device group" operation from Bobby.
        alice_ingest
            .process(IngestEvent {
                operation: create_bobby_device_01.clone(),
                args: IngestArgs {
                    log_id: LOG_ID.into(),
                    topic,
                    prune_flag: false,
                },
            })
            .await
            .unwrap();

        alice_groups
            .process(GroupsArgs::Process {
                state_id,
                operation: create_bobby_device_01.clone(),
            })
            .await
            .unwrap();

        // Then they create the chat group.
        let permit = alice_store.begin().await.unwrap();
        let y: GroupsState = alice_store
            .get_groups_state_tx(&state_id)
            .await
            .unwrap()
            .unwrap();
        alice_store.commit(permit).await.unwrap();

        let args = GroupsExtensionArgs {
            group_id: ab_chat,
            action: GroupAction::Create {
                initial_members: vec![
                    (GroupMember::Group(alice_device_group), Access::write()),
                    (GroupMember::Group(bobby_device_group), Access::write()),
                ],
            },
            dependencies: y.heads_filtered(&[alice_device_group, bobby_device_group]),
        };
        let create_alice_bobby_chat_03: Operation<TestExtensions> =
            alice_log.operation(&[], TestExtensions::from(args));

        alice_ingest
            .process(IngestEvent {
                operation: create_alice_bobby_chat_03.clone(),
                args: IngestArgs {
                    log_id: LOG_ID.into(),
                    topic,
                    prune_flag: false,
                },
            })
            .await
            .unwrap();

        alice_groups
            .process(GroupsArgs::Process {
                state_id,
                operation: create_alice_bobby_chat_03.clone(),
            })
            .await
            .unwrap();

        // Bobby processes alice's "create device group" and "create ab chat".
        for op in [create_alice_device_00.clone(), create_alice_bobby_chat_03] {
            bobby_ingest
                .process(IngestEvent {
                    operation: op.clone(),
                    args: IngestArgs {
                        log_id: LOG_ID.into(),
                        topic,
                        prune_flag: false,
                    },
                })
                .await
                .unwrap();
            bobby_groups
                .process(GroupsArgs::Process {
                    state_id,
                    operation: op,
                })
                .await
                .unwrap();
        }

        // Both Alice and Bobby have the correct groups state.
        for store in [alice_store.clone(), bobby_store.clone()] {
            let permit = store.begin().await.unwrap();
            let y: GroupsState = store.get_groups_state_tx(&state_id).await.unwrap().unwrap();
            store.commit(permit).await.unwrap();
            let mut members = y.members(ab_chat);
            members.sort();

            assert_eq!(members.len(), 2);
            assert!(members.contains(&(alice, Access::write())));
            assert!(members.contains(&(bobby, Access::write())));
        }

        // Cathy now creates a chat with Bobby.
        //
        // First they process "create device group" for bobby.
        cathy_ingest
            .process(IngestEvent {
                operation: create_bobby_device_01.clone(),
                args: IngestArgs {
                    log_id: LOG_ID.into(),
                    topic,
                    prune_flag: false,
                },
            })
            .await
            .unwrap();

        cathy_groups
            .process(GroupsArgs::Process {
                state_id,
                operation: create_bobby_device_01,
            })
            .await
            .unwrap();

        // Then they create the chat group.
        let permit = cathy_store.begin().await.unwrap();
        let y: GroupsState = cathy_store
            .get_groups_state_tx(&state_id)
            .await
            .unwrap()
            .unwrap();
        cathy_store.commit(permit).await.unwrap();
        let args = GroupsExtensionArgs {
            group_id: bc_chat,
            action: GroupAction::Create {
                initial_members: vec![
                    (GroupMember::Group(bobby_device_group), Access::write()),
                    (GroupMember::Group(cathy_device_group), Access::write()),
                ],
            },
            dependencies: y.heads_filtered(&[bobby_device_group, cathy_device_group]),
        };
        let create_bobby_cathy_chat_04: Operation<TestExtensions> =
            cathy_log.operation(&[], TestExtensions::from(args));

        cathy_ingest
            .process(IngestEvent {
                operation: create_bobby_cathy_chat_04.clone(),
                args: IngestArgs {
                    log_id: LOG_ID.into(),
                    topic,
                    prune_flag: false,
                },
            })
            .await
            .unwrap();

        cathy_groups
            .process(GroupsArgs::Process {
                state_id,
                operation: create_bobby_cathy_chat_04.clone(),
            })
            .await
            .unwrap();

        // Bobby processes cathy's "create device group" and "create bc chat".
        for op in [create_cathy_device_02.clone(), create_bobby_cathy_chat_04] {
            bobby_ingest
                .process(IngestEvent {
                    operation: op.clone(),
                    args: IngestArgs {
                        log_id: LOG_ID.into(),
                        topic,
                        prune_flag: false,
                    },
                })
                .await
                .unwrap();

            bobby_groups
                .process(GroupsArgs::Process {
                    state_id,
                    operation: op,
                })
                .await
                .unwrap();
        }

        // Both Cathy and Bobby have the correct groups state.
        for store in [cathy_store, bobby_store] {
            let permit = store.begin().await.unwrap();
            let y: GroupsState = store.get_groups_state_tx(&state_id).await.unwrap().unwrap();
            store.commit(permit).await.unwrap();
            let mut members = y.members(bc_chat);
            members.sort();

            assert_eq!(members.len(), 2);
            assert!(members.contains(&(bobby, Access::write())));
            assert!(members.contains(&(cathy, Access::write())));
        }
    }
}
