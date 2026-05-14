// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fmt::Debug;
use std::marker::PhantomData;

use p2panda_core::{Extension, Extensions, Hash, LogId, Operation, VerifyingKey};
use p2panda_store::groups::GroupsStore;
use p2panda_store::operations::OperationStore;
use p2panda_store::{SqliteError, SqliteStore, Transaction};
use p2panda_stream::ingest::{IngestError, ingest_operation};
use p2panda_stream::orderer::CausalOrderer;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{debug, trace};

use crate::group;
use crate::processor::{GroupsArgs, GroupsOperation};
use crate::traits::{Conditions, IdentityHandle, Operation as GroupsOperationTrait, OperationId};

type GroupsCrdt<C> = group::GroupCrdt<VerifyingKey, Hash, GroupsOperation<C>, C, StrongRemove<C>>;
type GroupsCrdtError<C> =
    group::GroupCrdtError<VerifyingKey, Hash, GroupsOperation<C>, C, StrongRemove<C>>;
type StrongRemove<C> = group::resolver::StrongRemove<VerifyingKey, Hash, GroupsOperation<C>, C>;
type GroupsState<C> = group::GroupCrdtState<VerifyingKey, Hash, GroupsOperation<C>, C>;

impl IdentityHandle for VerifyingKey {}
impl OperationId for Hash {}

/// Processor for groups operations.
#[derive(Clone)]
pub struct GroupsProcessor<T, E, L, C = ()> {
    store: SqliteStore,
    orderer: CausalOrderer<Hash, SqliteStore>,
    _marker: PhantomData<(T, E, L, C)>,
}

impl<T, E, L, C> GroupsProcessor<T, E, L, C>
where
    T: Serialize + for<'de> Deserialize<'de>,
    E: Extensions + Extension<GroupsArgs<C>> + Extension<L>,
    L: LogId,
    C: Conditions + Serialize + for<'a> Deserialize<'a>,
{
    pub fn new(store: SqliteStore) -> Self {
        Self {
            store: store.clone(),
            orderer: CausalOrderer::new(store),
            _marker: Default::default(),
        }
    }
    /// Process a groups operation.
    ///
    /// Processed operations are first partially ordered, and only processed on the auth groups
    /// state if all their dependencies have been met. If other operations become "ready" by this
    /// one, then they will be all processed in order.
    ///
    /// If an operation which does not contain the required groups extension is processed then it
    /// is ignored. Groups messages which are not yet present in the operation store are inserted.
    pub async fn process<SID>(
        &self,
        id: &SID,
        topic: &T,
        operation: &Operation<E>,
    ) -> Result<(), GroupsProcessorError<C>>
    where
        SID: for<'a> Deserialize<'a> + Serialize,
    {
        // ===== ingest ==== //

        // Extract the log id from the operation extensions.
        let Some(log_id): Option<L> = operation.header.extension() else {
            return Err(GroupsProcessorError::MissingLogId);
        };

        // Insert the operation to the store and form an association with the provided topic for
        // this author+log_id pair.
        ingest_operation(&self.store, &operation, &log_id, topic, false).await?;

        // Convert this Operation<E> into a GroupsOperation. If this returns None then the groups
        // extension was not present and so we consider this a non-groups operation which does not
        // require processing.
        let Some(args) = operation.header.extension::<GroupsArgs<C>>() else {
            trace!(id = operation.hash.to_hex(), "ignore non-groups operation");
            return Ok(());
        };

        let groups_operation = GroupsOperation {
            id: operation.hash,
            author: operation.header.verifying_key,
            dependencies: args.dependencies,
            group_id: args.group_id,
            action: args.action,
        };

        debug!(id = operation.hash.to_hex(), "process groups operation");

        // Start a transaction for all following database actions.
        let permit = self.store.begin().await?;

        // ==== ordering ==== //

        // Process the operation in the orderer.
        let operation_id = groups_operation.id();
        let dependencies = groups_operation.dependencies();
        self.orderer.process(operation_id, &dependencies).await?;

        // ==== groups ==== //

        // Retrieve the current groups state from the store.
        let mut y = GroupsStore::<SID, GroupsState<C>>::get_groups_state(&self.store, id)
            .await?
            .unwrap_or_default();

        debug!(
            group_id = groups_operation.group_id().to_hex(),
            "current group membership: {:?}",
            y.members(groups_operation.group_id())
        );

        // For all operations that are now "ready" (their dependencies have all been processed)
        // apply them to the groups state.
        while let Some(id) = self.orderer.next().await? {
            debug!(id = id.to_hex(), "apply ready operation to groups state");
            let groups_operation = get_groups_operation::<E, L, C>(&self.store, &id).await?;
            y = GroupsCrdt::process(y, &groups_operation)?;
        }

        // Set the groups state after processing is finished.
        self.store.set_groups_state(id, &y).await?;

        // Commit the open transaction.
        self.store.commit(permit).await?;

        debug!(
            group_id = groups_operation.group_id().to_hex(),
            "new group membership: {:?}",
            y.members(groups_operation.group_id())
        );

        Ok(())
    }
}

/// Get a groups operation from the operation store.
async fn get_groups_operation<E, L, C>(
    store: &SqliteStore,
    id: &Hash,
) -> Result<GroupsOperation<C>, GroupsProcessorError<C>>
where
    E: Extensions + Extension<GroupsArgs<C>> + Extension<L>,
    L: LogId,
    C: Conditions + Serialize + for<'a> Deserialize<'a>,
{
    let operation =
        <SqliteStore as OperationStore<Operation<E>, Hash, L>>::get_operation_tx(store, id).await?;

    let operation = match operation {
        Some(operation) => {
            let Some(args) = operation.header.extension::<GroupsArgs<C>>() else {
                return Err(GroupsProcessorError::<C>::MissingOperation(*id));
            };

            GroupsOperation {
                id: operation.hash,
                author: operation.header.verifying_key,
                dependencies: args.dependencies,
                group_id: args.group_id,
                action: args.action,
            }
        }
        None => {
            return Err(GroupsProcessorError::<C>::MissingOperation(*id));
        }
    };
    Ok(operation)
}

/// Error types which can occur in the groups processor.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Error)]
pub enum GroupsProcessorError<C>
where
    C: Conditions,
{
    #[error(transparent)]
    Store(#[from] SqliteError),

    #[error(transparent)]
    Groups(#[from] GroupsCrdtError<C>),

    #[error(transparent)]
    Ingest(#[from] IngestError),

    #[error("missing operation: {0}")]
    MissingOperation(Hash),

    #[error("operation retrieved from store missing groups arguments: {0}")]
    MissingGroupsArgs(Hash),

    #[error("missing \"log id\" operation extension")]
    MissingLogId,
}

#[cfg(test)]
mod tests {
    use p2panda_core::test_utils::TestLog;
    use p2panda_core::traits::Digest;
    use p2panda_core::{Extension, Hash, Header, Operation, SigningKey, Topic, VerifyingKey};
    use p2panda_store::groups::GroupsStore;
    use p2panda_store::{SqliteStore, Transaction};
    use serde::{Deserialize, Serialize};

    use crate::Access;
    use crate::group::{GroupAction, GroupCrdtState, GroupMember};
    use crate::processor::{GroupsArgs, GroupsOperation};
    use crate::test_utils::setup_logging;

    type LogId = u64;
    type GroupsState = GroupCrdtState<VerifyingKey, Hash, GroupsOperation, ()>;
    type GroupsProcessor = crate::processor::GroupsProcessor<Topic, TestExtensions, LogId>;

    const LOG_ID: u64 = 0;

    #[derive(Debug, Clone, Serialize, Deserialize)]
    struct TestExtensions {
        log_id: LogId,
        groups: Option<GroupsArgs>,
    }

    impl Extension<GroupsArgs> for TestExtensions {
        fn extract(header: &Header<Self>) -> Option<GroupsArgs> {
            header.extensions.groups.clone()
        }
    }

    impl Extension<LogId> for TestExtensions {
        fn extract(header: &Header<Self>) -> Option<LogId> {
            Some(header.extensions.log_id)
        }
    }

    impl From<GroupsArgs> for TestExtensions {
        fn from(args: GroupsArgs) -> Self {
            TestExtensions {
                groups: Some(args),
                log_id: LOG_ID,
            }
        }
    }

    #[tokio::test]
    async fn ooo_operations() {
        setup_logging();
        let topic = Topic::random();

        let alice_log = TestLog::new();
        let alice = alice_log.author();
        let bobby_log = TestLog::new();
        let bobby = bobby_log.author();
        let cathy_log = TestLog::new();
        let cathy = cathy_log.author();

        let state_id = 0;
        let group_id = SigningKey::generate().verifying_key();

        let args = GroupsArgs {
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

        let args = GroupsArgs {
            group_id,
            action: GroupAction::Add {
                member: GroupMember::Individual(cathy),
                access: Access::manage(),
            },
            dependencies: vec![op_00.hash()],
        };
        let op_01 = bobby_log.operation(&[], TestExtensions::from(args));

        let args = GroupsArgs {
            group_id,
            action: GroupAction::Remove {
                member: GroupMember::Individual(alice),
            },
            dependencies: vec![op_01.hash()],
        };
        let op_02 = cathy_log.operation(&[], TestExtensions::from(args));

        let store = SqliteStore::temporary().await;
        let groups = GroupsProcessor::new(store.clone());
        groups.process(&state_id, &topic, &op_02).await.unwrap();
        groups.process(&state_id, &topic, &op_01).await.unwrap();
        groups.process(&state_id, &topic, &op_00).await.unwrap();

        let permit = store.begin().await.unwrap();
        let y: GroupsState = store.get_groups_state(&state_id).await.unwrap().unwrap();
        store.commit(permit).await.unwrap();

        let members = y.members(group_id);
        assert_eq!(members.len(), 2);
        assert!(members.contains(&(bobby, Access::manage())));
        assert!(members.contains(&(cathy, Access::manage())));
    }

    #[tokio::test]
    async fn device_groups_single_context() {
        let topic = Topic::new();

        // All operations are processed on the same groups state context.
        let state_id = 'S';

        let alice_store = SqliteStore::temporary().await;
        let bobby_store = SqliteStore::temporary().await;
        let cathy_store = SqliteStore::temporary().await;

        let alice_log = TestLog::new();
        let alice = alice_log.author();
        let bobby_log = TestLog::new();
        let bobby = bobby_log.author();
        let cathy_log = TestLog::new();
        let cathy = cathy_log.author();

        let alice_device_group = PrivateKey::new().public_key();
        let bobby_device_group = PrivateKey::new().public_key();
        let cathy_device_group = PrivateKey::new().public_key();
        let ab_chat = PrivateKey::new().public_key();
        let bc_chat = PrivateKey::new().public_key();

        let alice_groups = GroupsProcessor::new(alice_store.clone());
        let bobby_groups = GroupsProcessor::new(bobby_store.clone());
        let cathy_groups = GroupsProcessor::new(cathy_store.clone());

        // All members create their own device groups and process them on their own stores.

        let args = GroupsArgs {
            group_id: alice_device_group,
            action: GroupAction::Create {
                initial_members: vec![(GroupMember::Individual(alice), Access::manage())],
            },
            dependencies: vec![],
        };
        let create_alice_device_00: Operation<TestExtensions> =
            alice_log.operation(&[], TestExtensions::from(args));

        alice_groups
            .process(&state_id, &topic, &create_alice_device_00)
            .await
            .unwrap();

        let args = GroupsArgs {
            group_id: bobby_device_group,
            action: GroupAction::Create {
                initial_members: vec![(GroupMember::Individual(bobby), Access::manage())],
            },
            dependencies: vec![],
        };
        let create_bobby_device_01: Operation<TestExtensions> =
            bobby_log.operation(&[], TestExtensions::from(args));

        bobby_groups
            .process(&state_id, &topic, &create_bobby_device_01)
            .await
            .unwrap();

        let args = GroupsArgs {
            group_id: cathy_device_group,
            action: GroupAction::Create {
                initial_members: vec![(GroupMember::Individual(cathy), Access::manage())],
            },
            dependencies: vec![],
        };
        let create_cathy_device_02: Operation<TestExtensions> =
            cathy_log.operation(&[], TestExtensions::from(args));

        cathy_groups
            .process(&state_id, &topic, &create_cathy_device_02)
            .await
            .unwrap();

        // Alice creates chat with Bobby.
        //
        // First they process "create device group" operation from Bobby.
        alice_groups
            .process(&state_id, &topic, &create_bobby_device_01)
            .await
            .unwrap();

        // Then they create the chat group.
        let permit = alice_store.begin().await.unwrap();
        let y: GroupsState = alice_store
            .get_groups_state(&state_id)
            .await
            .unwrap()
            .unwrap();
        alice_store.commit(permit).await.unwrap();

        let args = GroupsArgs {
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

        alice_groups
            .process(&state_id, &topic, &create_alice_bobby_chat_03)
            .await
            .unwrap();

        // Bobby processes alice's "create device group" and "create ab chat".
        for op in [create_alice_device_00.clone(), create_alice_bobby_chat_03] {
            bobby_groups.process(&state_id, &topic, &op).await.unwrap();
        }

        // Both Alice and Bobby have the correct groups state.
        for store in [alice_store.clone(), bobby_store.clone()] {
            let permit = store.begin().await.unwrap();
            let y: GroupsState = store.get_groups_state(&state_id).await.unwrap().unwrap();
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
        cathy_groups
            .process(&state_id, &topic, &create_bobby_device_01)
            .await
            .unwrap();

        // Then they create the chat group.
        let permit = cathy_store.begin().await.unwrap();
        let y: GroupsState = cathy_store
            .get_groups_state(&state_id)
            .await
            .unwrap()
            .unwrap();
        cathy_store.commit(permit).await.unwrap();
        let args = GroupsArgs {
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

        cathy_groups
            .process(&state_id, &topic, &create_bobby_cathy_chat_04)
            .await
            .unwrap();

        // Bobby processes cathy's "create device group" and "create bc chat".
        for op in [create_cathy_device_02.clone(), create_bobby_cathy_chat_04] {
            bobby_groups.process(&state_id, &topic, &op).await.unwrap();
        }

        // Both Cathy and Bobby have the correct groups state.
        for store in [cathy_store, bobby_store] {
            let permit = store.begin().await.unwrap();
            let y: GroupsState = store.get_groups_state(&state_id).await.unwrap().unwrap();
            store.commit(permit).await.unwrap();
            let mut members = y.members(bc_chat);
            members.sort();

            assert_eq!(members.len(), 2);
            assert!(members.contains(&(bobby, Access::write())));
            assert!(members.contains(&(cathy, Access::write())));
        }
    }
}
