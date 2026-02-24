// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fmt::{Debug, Display};
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
    pub async fn process(
        store: &Store<M, A, ID, C>,
        operation: &M,
    ) -> Result<(), GroupsProcessorError<M, A, ID, C>> {
        // Retrieve the current groups state from the store.
        //
        // @TODO: we will use the new store API here once upstream changes in `development` are
        // merged to main. This includes a transactional API which protects against potential race
        // conditions when multiple processes try to mutate the store concurrently. For now we use
        // an in-memory store with a Mutex to coordinate shared access.
        let _lock = store.begin_transaction().await;
        let mut y = store.take_state().await;

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
        store.set_state(y).await;

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
        GroupsProcessor::process(&groups_store, &op_02)
            .await
            .unwrap();
        GroupsProcessor::process(&groups_store, &op_01)
            .await
            .unwrap();
        GroupsProcessor::process(&groups_store, &op_00)
            .await
            .unwrap();
    }
}
