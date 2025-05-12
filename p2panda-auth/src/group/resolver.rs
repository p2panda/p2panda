use std::collections::{HashMap, HashSet};
use std::fmt::Display;
use std::{fmt::Debug, marker::PhantomData};

use petgraph::algo::toposort;
use petgraph::prelude::DiGraphMap;
use thiserror::Error;

use crate::group::{GroupControlMessage, GroupMember, GroupState};
use crate::traits::{GroupStore, IdentityHandle, Operation, OperationId, Ordering, Resolver};

use super::{GroupAction, GroupStateInner};

// TODO: introduce all error types.
#[derive(Debug, Error)]
pub enum GroupResolverError {}

/// Resolver for group membership auth graph.
#[derive(Clone, Debug, Default)]
pub struct GroupResolver<ID, OP, C, ORD, GS> {
    _phantom: PhantomData<(ID, OP, C, ORD, GS)>,
}

impl<ID, OP, C, ORD, GS> Resolver<ORD::Message> for GroupResolver<ID, OP, C, ORD, GS>
where
    ID: IdentityHandle + Display,
    OP: OperationId + Display + Ord,
    C: Clone + Debug + PartialEq + PartialOrd,
    ORD: Ordering<ID, OP, GroupControlMessage<ID, OP, C>> + Clone + Debug,
    ORD::Message: Clone,
    ORD::State: Clone,
    GS: GroupStore<ID, OP, C, Self, ORD> + Debug,
{
    type State = GroupState<ID, OP, C, Self, ORD, GS>;
    type Error = GroupResolverError;

    fn rebuild_required(y: &Self::State, operation: &ORD::Message) -> bool {
        let control_message = operation.payload();
        let actor = operation.sender();

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

        // TODO: Get concurrent branches.
        // let concurrent_operations = get_concurrent(operation: &OP, graph: &y.inner.graph);

        match operation.payload() {
            GroupControlMessage::Revoke { .. } => {
                // Any revoke message requires a re-build.
                true
            }
            GroupControlMessage::GroupAction { .. } => {
                if is_concurrent {
                    match action {
                        GroupAction::Remove { member } => {
                            // Optional optimization to avoid unnecessary re-builds, only return
                            // true if:
                            // 1) The removed member performed an admin action in any concurrent
                            //    branch && they actually were an admin.
                            // 2) ..?

                            true
                        }
                        GroupAction::Demote { member, access } => {
                            // Optional optimizations to avoid unnecessary re-builds, only return
                            // true if:
                            // 1) The demoted member was previously an admin && they performed an
                            //    admin action in a concurrent branch.
                            // 2) The demoted member was promoted to admin in a concurrent branch
                            //    && they performed an admin action.
                            // 3) ..?

                            true
                        }
                        _ => {
                            // TODO: Check if there are any concurrent actions which invalidate this
                            // action. If there are we could actually invalidate it immediately,
                            // maybe this method should return a state object as well as the boolean.
                            false
                        }
                    }
                } else {
                    false
                }
            }
        }
    }

    fn process(
        mut y: GroupState<ID, OP, Self, ORD, GS>,
    ) -> Result<GroupState<ID, OP, Self, ORD, GS>, Self::Error> {
        // Calculate a map of operations to a "bubble" of all operations concurrent to them.
        fn get_concurrent_bubbles<OP>(graph: &DiGraphMap<OP, ()>) -> HashMap<OP, Vec<OP>> {
            // example: https://github.com/p2panda/access-control-playground/blob/e552e5eef90bc9e05bb4c96b2ac9ee7d694b0afa/004_petgraph-reduce-graph-with-filter/src/main.rs#L8
            todo!()
        }

        // All bubbles present in this graph.
        let bubbles = get_concurrent_bubbles(&y.inner.graph);

        // A new set of operations to be filtered which we will now populate.
        let mut filter: HashSet<OP> = Default::default();

        // Iterate over all bubbles, apply membership rules and populate the filter accordingly.
        for (operation, bubble) in bubbles {
            // Steps based on auth membership rules: https://github.com/local-first-web/auth/blob/f61e3678d74f9a30946475941ef9ef0c8c45d664/packages/auth/src/team/membershipResolver.ts#L83
            //
            // NOTE: we made some different decisions about how to resolve conflicts, but
            // how to understand what constitutes a conflict is still useful to follow.

            // 1) Mutual removals
            //
            // In our first resolve strategy mutual removals result in both members being removed from
            // the group. We imagine further implementations taking different approaches, like
            // resolving by seniority, hash id, quorum or some other parameter.

            // 2) Re-adding member concurrently
            //
            // We don't stop this behaviour, if A removes C and B removes then adds C concurrently, C is still
            // in the group.

            // 3) Removed admin performing concurrent actions
            //
            // If A removes B, then B shouldn't be able to perform any actions concurrently.

            // 4) Demoted admin performing concurrent actions
            //
            // If A demotes B (from admin), then B shouldn't be able to perform any actions concurrently.
        }

        // Set the new "ignore filter".
        y.inner.ignore = filter;

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
        type AuthResolver<ORD, GS> = GroupResolver<MemberId, MessageId, (), ORD, GS>;
        type AuthGroup<ORD, GS> = Group<MemberId, MessageId, (), AuthResolver<ORD, GS>, ORD, GS>;
        type AuthGroupState<RS, ORD, GS> = GroupState<MemberId, MessageId, (), RS, ORD, GS>;

        let rng = StdRng::from_os_rng();
        let store = TestGroupStore::default();
        let orderer_y = TestOrdererState::new('A', store.clone(), rng);
        let group_y: AuthGroupState<TestResolver, TestOrderer, TestGroupStore> =
            AuthGroupState::new('A', 'B', store.clone(), orderer_y);
        let _group_y_i = AuthGroup::rebuild(group_y).unwrap();
    }
}
