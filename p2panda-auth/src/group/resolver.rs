use std::collections::HashSet;
use std::fmt::Display;
use std::{fmt::Debug, marker::PhantomData};

use thiserror::Error;

use crate::group::{GroupControlMessage, GroupState};
use crate::traits::{GroupStore, IdentityHandle, Operation, OperationId, Ordering, Resolver};

use super::GroupStateInner;

// TODO: introduce all error types.
#[derive(Debug, Error)]
pub enum GroupResolverError {}

/// Resolver for group membership auth graph.
#[derive(Clone, Debug, Default)]
pub struct GroupResolver<ID, OP, MSG> {
    _phantom: PhantomData<(ID, OP, MSG)>,
}

impl<ID, OP, ORD, GS> Resolver<GroupState<ID, OP, Self, ORD, GS>, ORD::Message>
    for GroupResolver<ID, OP, ORD::Message>
where
    ID: IdentityHandle + Display,
    OP: OperationId + Display + Ord,
    ORD: Clone + Debug + Ordering<ID, OP, GroupControlMessage<ID, OP>>,
    GS: Clone + Debug + GroupStore<ID, GroupStateInner<ID, OP, ORD::Message>>,
{
    type Error = GroupResolverError;

    fn rebuild_required(y: &GroupState<ID, OP, Self, ORD, GS>, operation: &ORD::Message) -> bool {
        let control_message = operation.payload();

        // Sanity check.
        if control_message.group_id() != y.inner.group_id {
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
            GroupControlMessage::GroupAction { action, .. } => {
                if is_concurrent {
                    match action {
                        // TODO: Implement logic for detecting when concurrent actions should
                        // trigger a re-build.
                        _ => false,
                    }
                } else {
                    false
                }
            }
        }
    }

    fn process(
        y: GroupState<ID, OP, Self, ORD, GS>,
    ) -> Result<GroupState<ID, OP, Self, ORD, GS>, Self::Error> {
        // TODO: Implement resolver logic.
        Ok(y)
    }
}
