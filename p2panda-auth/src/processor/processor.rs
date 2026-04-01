// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fmt::Debug;
use std::marker::PhantomData;

use p2panda_core::{Extension, Extensions, Hash, LogId, Operation, PublicKey};
use p2panda_store::groups::GroupsStore;
use p2panda_store::operations::OperationStore;
use p2panda_store::{SqliteError, SqliteStore, Transaction};
use p2panda_stream::orderer::CausalOrderer;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::debug;

use crate::group::{GroupCrdt, GroupCrdtState};
use crate::processor::args::GroupsArgsError;
use crate::processor::{GroupsArgs, GroupsOperation};
use crate::traits::{Conditions, IdentityHandle, Operation as GroupsOperationTrait, OperationId};

type GroupsCrdtError<C> =
    crate::group::GroupCrdtError<PublicKey, Hash, GroupsOperation<C>, C, StrongRemove<C>>;
type StrongRemove<C> = crate::group::resolver::StrongRemove<PublicKey, Hash, GroupsOperation<C>, C>;
type GroupsState<C> = GroupCrdtState<PublicKey, Hash, GroupsOperation<C>, C>;

impl IdentityHandle for PublicKey {}
impl OperationId for Hash {}

/// Processor for groups operations.
#[derive(Clone)]
pub struct GroupsProcessor<E, L, C = ()> {
    _marker: PhantomData<(E, L, C)>,
}

impl<E, L, C> GroupsProcessor<E, L, C>
where
    E: Extensions + Extension<GroupsArgs<C>> + Extension<L>,
    L: LogId,
    C: Conditions + Serialize + for<'a> Deserialize<'a>,
{
    /// Process a groups operation.
    ///
    /// Processed operations are first partially ordered, and only processed on the auth groups
    /// state if all their dependencies have been met. If other operations become "ready" by this
    /// one, then they will be all processed in order.
    ///
    /// If an operation which does not contain the required groups extension is processed then it
    /// is ignored. Groups messages which are not yet present in the operation store are inserted.
    pub async fn process<SID>(
        id: &SID,
        store: &SqliteStore,
        operation: &Operation<E>,
    ) -> Result<(), GroupsProcessorError<C>>
    where
        SID: for<'a> Deserialize<'a> + Serialize,
    {
        // Convert this Operation<E> into a GroupsOperation. If this fails then the groups
        // extension was not present and so we consider this a non-groups operation which does not
        // require processing.
        let Ok(groups_operation): Result<GroupsOperation<C>, _> = operation.clone().try_into()
        else {
            // @TODO: should this be an error rather than silently ignoring non-groups operations?
            debug!(id = operation.hash.to_hex(), "ignore non-groups operation");
            return Ok(());
        };

        debug!(id = operation.hash.to_hex(), "process groups operation");

        // Check if the operation is already in the store, if it isn't then we'll insert it
        // ourselves.
        insert_operation_checked::<E, L, C>(&store, &operation).await?;

        // Start a transaction for all following database actions.
        let permit = store.begin().await?;

        // Retrieve the current groups state from the store.
        let mut y = GroupsStore::<SID, GroupsState<C>>::get_state(store, id)
            .await?
            .unwrap_or_default();

        debug!(
            group_id = groups_operation.group_id().to_hex(),
            "current group membership: {:?}",
            y.members(groups_operation.group_id())
        );

        // Process the operation in the orderer.
        let operation_id = groups_operation.id();
        let dependencies = groups_operation.dependencies();
        let mut orderer = CausalOrderer::new(store.clone());
        orderer.process(operation_id, &dependencies).await?;

        // For all operations that are now "ready" (their dependencies have all been processed)
        // apply them to the groups state.
        while let Some(id) = orderer.next().await? {
            debug!(id = id.to_hex(), "apply ready operation to groups state");
            let groups_operation = get_groups_operation::<E, L, C>(&store, &id).await?;
            y = GroupCrdt::<_, _, _, C, StrongRemove<C>>::process(y, &groups_operation)?;
        }

        // Set the groups state after processing is finished.
        store.set_state(id, &y).await?;

        // Commit the open transaction.
        store.commit(permit).await?;

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
        Some(operation) => { operation as Operation<E> }.try_into()?,
        None => {
            return Err(GroupsProcessorError::<C>::MissingOperation(*id));
        }
    };
    Ok(operation)
}

/// Insert an operation to the store if it isn't already present.
async fn insert_operation_checked<E, L, C>(
    store: &SqliteStore,
    operation: &Operation<E>,
) -> Result<(), GroupsProcessorError<C>>
where
    E: Extensions + Extension<GroupsArgs<C>> + Extension<L>,
    L: LogId,
    C: Conditions + Serialize + for<'a> Deserialize<'a>,
{
    // Check if the operation is already in the store.
    let has_operation = <SqliteStore as OperationStore<Operation<E>, Hash, L>>::has_operation(
        store,
        &operation.hash,
    )
    .await?;

    // Insert operation to store if not present.
    if !has_operation {
        let Some(log_id): Option<L> = operation.header.extension() else {
            return Err(GroupsProcessorError::MissingLogId);
        };

        let permit = store.begin().await?;

        <SqliteStore as OperationStore<Operation<E>, Hash, L>>::insert_operation(
            store,
            &operation.hash,
            &operation,
            &log_id,
        )
        .await?;

        store.commit(permit).await?;

        debug!(id = operation.hash.to_hex(), "operation inserted to store");
    };

    Ok(())
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

    #[error("missing operation: {0}")]
    MissingOperation(Hash),

    #[error(transparent)]
    MissingGroupsArgs(#[from] GroupsArgsError),

    #[error("missing \"log id\" operation extension")]
    MissingLogId,
}

#[cfg(test)]
mod tests {
    use p2panda_core::test_utils::TestLog;
    use p2panda_core::traits::Digest;
    use p2panda_core::{Extension, Hash, Header, Operation, PrivateKey, PublicKey};
    use p2panda_store::groups::GroupsStore;
    use p2panda_store::{SqliteStore, Transaction};
    use serde::{Deserialize, Serialize};

    use crate::Access;
    use crate::group::{GroupAction, GroupCrdtState, GroupMember};
    use crate::processor::{GroupsArgs, GroupsOperation};
    use crate::test_utils::setup_logging;

    type LogId = u64;
    type GroupsState = GroupCrdtState<PublicKey, Hash, GroupsOperation, ()>;
    type GroupsProcessor = crate::processor::GroupsProcessor<TestExtensions, LogId>;

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

        let alice_log = TestLog::new();
        let alice = alice_log.author();
        let bobby_log = TestLog::new();
        let bobby = bobby_log.author();
        let cathy_log = TestLog::new();
        let cathy = cathy_log.author();

        let state_id = 0;
        let group_id = PrivateKey::new().public_key();

        let args = GroupsArgs {
            group_id,
            action: GroupAction::Create {
                initial_members: vec![(GroupMember::Individual(alice), <Access>::manage())],
            },
            dependencies: vec![],
        };
        let op_00: Operation<TestExtensions> = alice_log.operation(&[], TestExtensions::from(args));

        let args = GroupsArgs {
            group_id,
            action: GroupAction::Add {
                member: GroupMember::Individual(bobby),
                access: Access::manage(),
            },
            dependencies: vec![op_00.hash()],
        };
        let op_01 = alice_log.operation(&[], TestExtensions::from(args));

        let args = GroupsArgs {
            group_id,
            action: GroupAction::Add {
                member: GroupMember::Individual(cathy),
                access: Access::manage(),
            },
            dependencies: vec![op_01.hash()],
        };
        let op_02 = alice_log.operation(&[], TestExtensions::from(args));

        let store = SqliteStore::temporary().await;
        GroupsProcessor::process(&state_id, &store, &op_02)
            .await
            .unwrap();
        GroupsProcessor::process(&state_id, &store, &op_01)
            .await
            .unwrap();
        GroupsProcessor::process(&state_id, &store, &op_00)
            .await
            .unwrap();

        let permit = store.begin().await.unwrap();
        let y: GroupsState = store.get_state(&state_id).await.unwrap().unwrap();
        store.commit(permit).await.unwrap();

        let members = y.members(group_id);
        assert_eq!(members.len(), 3);
        assert!(members.contains(&(alice, Access::manage())));
        assert!(members.contains(&(bobby, Access::manage())));
        assert!(members.contains(&(cathy, Access::manage())));
    }

    #[tokio::test]
    async fn device_groups_single_context() {
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

        GroupsProcessor::process(&state_id, &alice_store, &create_alice_device_00)
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

        GroupsProcessor::process(&state_id, &bobby_store, &create_bobby_device_01)
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

        GroupsProcessor::process(&state_id, &cathy_store, &create_cathy_device_02)
            .await
            .unwrap();

        // Alice creates chat with Bobby.
        //
        // First they process "create device group" operation from Bobby.
        GroupsProcessor::process(&state_id, &alice_store, &create_bobby_device_01)
            .await
            .unwrap();

        // Then they create the chat group.
        let permit = alice_store.begin().await.unwrap();
        let y: GroupsState = alice_store.get_state(&state_id).await.unwrap().unwrap();
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

        GroupsProcessor::process(&state_id, &alice_store, &create_alice_bobby_chat_03)
            .await
            .unwrap();

        // Bobby processes alice's "create device group" and "create ab chat".
        for op in [create_alice_device_00.clone(), create_alice_bobby_chat_03] {
            GroupsProcessor::process(&state_id, &bobby_store, &op)
                .await
                .unwrap();
        }

        // Both Alice and Bobby have the correct groups state.
        for store in [alice_store.clone(), bobby_store.clone()] {
            let permit = store.begin().await.unwrap();
            let y: GroupsState = store.get_state(&state_id).await.unwrap().unwrap();
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
        GroupsProcessor::process(&state_id, &cathy_store, &create_bobby_device_01)
            .await
            .unwrap();

        // Then they create the chat group.
        let permit = cathy_store.begin().await.unwrap();
        let y: GroupsState = cathy_store.get_state(&state_id).await.unwrap().unwrap();
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

        GroupsProcessor::process(&state_id, &cathy_store, &create_bobby_cathy_chat_04)
            .await
            .unwrap();

        // Bobby processes cathy's "create device group" and "create bc chat".
        for op in [create_cathy_device_02.clone(), create_bobby_cathy_chat_04] {
            GroupsProcessor::process(&state_id, &bobby_store, &op)
                .await
                .unwrap();
        }

        // Both Cathy and Bobby have the correct groups state.
        for store in [cathy_store, bobby_store] {
            let permit = store.begin().await.unwrap();
            let y: GroupsState = store.get_state(&state_id).await.unwrap().unwrap();
            store.commit(permit).await.unwrap();
            let mut members = y.members(bc_chat);
            members.sort();

            assert_eq!(members.len(), 2);
            assert!(members.contains(&(bobby, Access::write())));
            assert!(members.contains(&(cathy, Access::write())));
        }
    }
}
