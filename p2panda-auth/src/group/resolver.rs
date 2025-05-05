use std::fmt::Display;
use std::{fmt::Debug, marker::PhantomData};

use thiserror::Error;

use crate::group::{GroupControlMessage, GroupState};
use crate::traits::{IdentityHandle, Operation, OperationId, Ordering, Resolver};

// TODO: introduce all error types.
#[derive(Debug, Error)]
pub enum GroupResolverError {}

/// Resolver for group membership auth graph.
#[derive(Clone, Debug, Default)]
pub struct GroupResolver<ID, OP, MSG> {
    _phantom: PhantomData<(ID, OP, MSG)>,
}

impl<ID, OP, ORD> Resolver<GroupState<ID, OP, ORD>, ORD::Message>
    for GroupResolver<ID, OP, ORD::Message>
where
    ID: IdentityHandle + Display,
    OP: OperationId + Ord + Display,
    ORD: Ordering<ID, OP, GroupControlMessage<ID, OP>>,
{
    type Error = GroupResolverError;

    fn rebuild_required(y: &GroupState<ID, OP, ORD>, operation: &ORD::Message) -> bool {
        let control_message = operation.payload();

        // Get the group id from the control message.
        let group_id = match control_message {
            GroupControlMessage::GroupAction { group_id, .. } => group_id,
            GroupControlMessage::Revoke { group_id, .. } => group_id,
        };

        // Sanity check.
        if *group_id != y.group_id {
            panic!();
        }

        // Get all current tip operations.
        let heads = y.heads();

        // Detect concurrent operations by comparing the current heads with the new operations
        // dependencies.
        let is_concurrent = &heads != operation.dependencies();

        match operation.payload() {
            GroupControlMessage::Revoke { .. } => {
                // Any revoke message requires a re-build.
                true
            }
            GroupControlMessage::GroupAction { group_id, action } => {
                if is_concurrent {
                    match action {
                        // TODO: Decide which (if any) concurrent actions cause a rebuild.
                        _ => false,
                    }
                } else {
                    false
                }
            }
        }
    }

    fn process(y: GroupState<ID, OP, ORD>) -> Result<GroupState<ID, OP, ORD>, Self::Error> {
        // TODO: We don't construct any filter, this is where that logic should be implemented.
        Ok(y)
    }
}
