// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fmt::{Debug, Display};
use std::hash::Hash as StdHash;
use std::marker::PhantomData;

use p2panda_core::{Hash, PublicKey};
use p2panda_stream::partial::{PartialOrder, PartialOrderError};
use thiserror::Error;

use crate::group::resolver::StrongRemove;
use crate::group::{GroupCrdt, GroupCrdtError};
use crate::processor::Store;
use crate::traits::{Conditions, IdentityHandle, Operation as GroupsOperation, OperationId};

impl IdentityHandle for PublicKey {}
impl OperationId for Hash {}

/// Processor for groups operations.
#[derive(Clone)]
pub struct GroupsProcessor<M, A = PublicKey, ID = Hash, C = ()>
where
    M: GroupsOperation<A, ID, C>,
    A: IdentityHandle,
    ID: OperationId + Display,
    C: Conditions,
{
    _marker: PhantomData<(M, A, ID, C)>,
}

impl<M, A, ID, C> GroupsProcessor<M, A, ID, C>
where
    M: GroupsOperation<A, ID, C> + Clone + Debug,
    A: IdentityHandle,
    ID: OperationId + Display,
    C: Conditions,
{
    /// Process a groups operation.
    ///
    /// Processed operations are first partially ordered, and only processed on the auth groups state
    /// if all their dependencies have been met. If other operations become "ready" by this one, then
    /// they will be all processed in order.
    pub async fn process<SID>(
        id: &SID,
        store: &Store<SID, M, A, ID, C>,
        operation: &M,
    ) -> Result<(), GroupsProcessorError<M, A, ID, C>>
    where
        SID: Copy + Eq + StdHash,
    {
        // Retrieve the current groups state from the store.
        //
        // @TODO: we will use the new store API here once upstream changes in `development` are
        // merged to main. This includes a transactional API which protects against potential race
        // conditions when multiple processes try to mutate the store concurrently. For now we use
        // an in-memory store with a Mutex to coordinate shared access.
        let _lock = store.begin_transaction().await;
        let mut y = store.get_state(id).await.unwrap_or_default();

        // Add operation to the internal buffer.
        let operation_id = operation.id();
        let dependencies = operation.dependencies();
        y.operation_buffer.insert(operation_id, operation.clone());

        // Process the operation in the orderer.
        let mut orderer = PartialOrder::new(y.orderer.clone());
        orderer.process(operation_id, &dependencies).await?;

        // For all operations that are now "ready" (their dependencies have all been processed)
        // apply them to the groups state.
        while let Some(hash) = orderer.next().await? {
            let operation = match y.operation_buffer.remove(&hash) {
                Some(operation) => operation,
                None => {
                    return Err(GroupsProcessorError::MissingOperation(hash));
                }
            };

            y.crdt =
                GroupCrdt::<_, _, _, C, StrongRemove<A, ID, M, C>>::process(y.crdt, &operation)?;
        }

        // Set the groups state after processing is finished.
        y.orderer = orderer.store();
        store.set_state(id, y).await;

        Ok(())
    }
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Error)]
pub enum GroupsProcessorError<M, A, ID, C>
where
    A: IdentityHandle,
    ID: OperationId,
    M: GroupsOperation<A, ID, C> + Clone + Debug,
    C: Conditions,
{
    #[error(transparent)]
    Orderer(#[from] PartialOrderError),

    #[error(transparent)]
    Groups(#[from] GroupCrdtError<A, ID, M, C, StrongRemove<A, ID, M, C>>),

    #[error("missing operation: {0}")]
    MissingOperation(ID),
}

#[cfg(test)]
mod tests {
    use crate::Access;
    use crate::group::GroupMember;
    use crate::processor::{GroupsProcessor, Store};
    use crate::test_utils::{add_member, create_group};
    use crate::traits::Operation;

    #[tokio::test]
    async fn ooo_operations() {
        let chat_id = 0;
        let group_id = 'G';

        let alice = 'A';
        let bobby = 'B';
        let cathy = 'C';

        let op_00 = create_group(
            alice,
            0,
            group_id,
            vec![(GroupMember::Individual(alice), Access::manage())],
            vec![],
        );

        let op_01 = add_member(
            alice,
            1,
            group_id,
            GroupMember::Individual(bobby),
            Access::manage(),
            vec![op_00.id()],
        );

        let op_02 = add_member(
            alice,
            2,
            group_id,
            GroupMember::Individual(cathy),
            Access::read(),
            vec![op_01.id()],
        );

        let groups_store = Store::default();
        GroupsProcessor::process(&chat_id, &groups_store, &op_02)
            .await
            .unwrap();
        GroupsProcessor::process(&chat_id, &groups_store, &op_01)
            .await
            .unwrap();
        GroupsProcessor::process(&chat_id, &groups_store, &op_00)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn device_groups() {
        let alice_store = Store::default();
        let bobby_store = Store::default();
        let cathy_store = Store::default();

        let alice = 'A';
        let bobby = 'B';
        let cathy = 'C';
        let alice_device_group = 'X';
        let bobby_device_group = 'Y';
        let cathy_device_group = 'Z';
        let ab_chat = '0';
        let bc_chat = '1';

        // All members create their own device groups and process them on their own stores.

        let create_alice_device_00 = create_group(
            alice,
            0,
            alice_device_group,
            vec![(GroupMember::Individual(alice), Access::manage())],
            vec![],
        );

        GroupsProcessor::process(&alice_device_group, &alice_store, &create_alice_device_00)
            .await
            .unwrap();

        let create_bobby_device_01 = create_group(
            bobby,
            1,
            bobby_device_group,
            vec![(GroupMember::Individual(bobby), Access::manage())],
            vec![],
        );

        GroupsProcessor::process(&bobby_device_group, &bobby_store, &create_bobby_device_01)
            .await
            .unwrap();

        let create_cathy_device_02 = create_group(
            alice,
            2,
            cathy_device_group,
            vec![(GroupMember::Individual(cathy), Access::manage())],
            vec![],
        );

        GroupsProcessor::process(&cathy_device_group, &cathy_store, &create_cathy_device_02)
            .await
            .unwrap();

        // Alice creates chat with Bobby.
        //
        // First they process "create device group" operations for themselves and bobby on a new
        // groups context.
        for op in [
            create_alice_device_00.clone(),
            create_bobby_device_01.clone(),
        ] {
            GroupsProcessor::process(&ab_chat, &alice_store, &op)
                .await
                .unwrap();
        }

        // Then they create the chat group.
        let ab_groups_context = alice_store.get_state(&ab_chat).await.unwrap();
        let create_alice_bobby_chat_03 = create_group(
            alice,
            3,
            ab_chat,
            vec![
                (GroupMember::Group(alice_device_group), Access::write()),
                (GroupMember::Group(bobby_device_group), Access::write()),
            ],
            ab_groups_context.crdt.heads(),
        );

        GroupsProcessor::process(&ab_chat, &alice_store, &create_alice_bobby_chat_03)
            .await
            .unwrap();

        // Bobby processes all operations in the "ab_chat" context.
        for op in [
            create_alice_device_00.clone(),
            create_bobby_device_01.clone(),
            create_alice_bobby_chat_03,
        ] {
            GroupsProcessor::process(&ab_chat, &bobby_store, &op)
                .await
                .unwrap();
        }

        // Both Alice and Bobby have the correct groups state.
        for store in [alice_store.clone(), bobby_store.clone()] {
            let mut members = store
                .get_state(&ab_chat)
                .await
                .unwrap()
                .crdt
                .members(ab_chat);
            members.sort();

            assert_eq!(
                members,
                vec![('A', Access::write()), ('B', Access::write())]
            );
        }

        // Cathy now creates a chat with Bobby.
        //
        // First they process "create device group" operations for themselves and bobby on a new
        // groups context.
        for op in [
            create_bobby_device_01.clone(),
            create_cathy_device_02.clone(),
        ] {
            GroupsProcessor::process(&bc_chat, &cathy_store, &op)
                .await
                .unwrap();
        }

        // Then they create the chat group.
        let bc_groups_context = cathy_store.get_state(&bc_chat).await.unwrap();
        let create_bobby_cathy_chat_04 = create_group(
            cathy,
            4,
            bc_chat,
            vec![
                (GroupMember::Group(bobby_device_group), Access::write()),
                (GroupMember::Group(cathy_device_group), Access::write()),
            ],
            bc_groups_context.crdt.heads(),
        );

        GroupsProcessor::process(&bc_chat, &cathy_store, &create_bobby_cathy_chat_04)
            .await
            .unwrap();

        // Bobby processes all operations in the "bc_chat" context.
        for op in [
            create_bobby_device_01.clone(),
            create_cathy_device_02.clone(),
            create_bobby_cathy_chat_04,
        ] {
            GroupsProcessor::process(&bc_chat, &bobby_store, &op)
                .await
                .unwrap();
        }

        // Both Cathy and Bobby have the correct groups state.
        for store in [cathy_store, bobby_store] {
            let mut members = store
                .get_state(&bc_chat)
                .await
                .unwrap()
                .crdt
                .members(bc_chat);
            members.sort();

            assert_eq!(
                members,
                vec![('B', Access::write()), ('C', Access::write())]
            );
        }
    }
}
