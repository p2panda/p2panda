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
    ID: IdentityHandle,
    OP: OperationId + Ord,
    ORD: Ordering<ID, OP, GroupControlMessage<ID, OP>>,
{
    type Error = GroupResolverError;

    fn rebuild_required(y: &GroupState<ID, OP, ORD>, message: &ORD::Message) -> bool {
        // Get all current tip operations.
        let heads = y.heads();

        // Detect concurrent operations by comparing the current heads with the new operations
        // dependencies.
        let is_concurrent = &heads != message.dependencies();

        match message.payload() {
            GroupControlMessage::Revoke { .. } => {
                // Any revoke message requires a re-build.
                true
            }
            GroupControlMessage::GroupAction(action) => {
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
