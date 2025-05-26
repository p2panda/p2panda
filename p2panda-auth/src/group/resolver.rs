// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashSet;
use std::fmt::Display;
use std::{fmt::Debug, marker::PhantomData};

use thiserror::Error;

use crate::group::{GroupControlMessage, GroupState};
use crate::traits::{GroupStore, IdentityHandle, Operation, OperationId, Ordering, Resolver};

// TODO: introduce all error types.
#[derive(Debug, Error)]
pub enum GroupResolverError {}

/// Resolver for group membership auth graph.
#[derive(Clone, Debug, Default)]
pub struct GroupResolver<ID, OP, ORD, GS> {
    _phantom: PhantomData<(ID, OP, ORD, GS)>,
}

impl<ID, OP, ORD, GS> Resolver<ORD::Message> for GroupResolver<ID, OP, ORD, GS>
where
    ID: IdentityHandle + Display,
    OP: OperationId + Display + Ord,
    ORD: Ordering<ID, OP, GroupControlMessage<ID, OP>> + Clone + Debug,
    ORD::Message: Clone,
    ORD::State: Clone,
    GS: GroupStore<ID, Group = GroupState<ID, OP, Self, ORD, GS>> + Debug,
{
    type State = GroupState<ID, OP, Self, ORD, GS>;
    type Error = GroupResolverError;

    fn rebuild_required(y: &Self::State, operation: &ORD::Message) -> bool {
        let control_message = operation.payload();

        // Sanity check.
        if control_message.group_id() != y.group_id {
            panic!();
        }

        // Get all current tip operations.
        //
        // TODO: should be checking against transitive heads here.
        let heads = y.heads();

        // Detect concurrent operations by comparing the current heads with the new operations'
        // dependencies.
        let is_concurrent = heads != HashSet::from_iter(operation.dependencies().clone());

        match operation.payload() {
            GroupControlMessage::Revoke { .. } => {
                // Any revoke message requires a re-build.
                true
            }
            GroupControlMessage::GroupAction { .. } => {
                if is_concurrent {
                    // TODO: Implement logic for detecting when concurrent actions should
                    // trigger a re-build.
                    false
                } else {
                    false
                }
            }
        }
    }

    fn process(y: Self::State) -> Result<Self::State, Self::Error> {
        // TODO: Implement resolver logic.
        Ok(y)
    }
}

#[cfg(test)]
mod tests {
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    use crate::group::resolver::GroupResolver;
    use crate::group::test_utils::{
        MemberId, MessageId, TestGroupStore, TestOrderer, TestOrdererState, TestResolver,
    };
    use crate::group::{Group, GroupState};

    #[test]
    fn trait_definition_not_recursive() {
        type AuthResolver<ORD, GS> = GroupResolver<MemberId, MessageId, ORD, GS>;
        type AuthGroup<ORD, GS> = Group<MemberId, MessageId, AuthResolver<ORD, GS>, ORD, GS>;
        type AuthGroupState<RS, ORD, GS> = GroupState<MemberId, MessageId, RS, ORD, GS>;

        let rng = StdRng::from_os_rng();
        let store = TestGroupStore::default();
        let orderer_y = TestOrdererState::new('A', store.clone(), rng);
        let group_y: AuthGroupState<TestResolver, TestOrderer, TestGroupStore<MemberId>> =
            AuthGroupState::new('A', 'B', store.clone(), orderer_y);
        let _group_y_i = AuthGroup::rebuild(group_y).unwrap();
    }
}
