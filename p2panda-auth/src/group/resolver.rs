use std::{fmt::Debug, hash::Hash, marker::PhantomData};

use petgraph::visit::NodeIndexable;
use thiserror::Error;

use crate::group::{GroupControlMessage, GroupState};
use crate::traits::{Operation, Ordering, Resolver};

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
    ID: Clone + Copy + Debug + Eq + Hash,
    OP: Clone + Copy + Ord + Debug + Eq + Hash,
    ORD: Ordering<ID, OP, GroupControlMessage<ID, OP>>,
    ORD::Message: Debug,
{
    type Error = GroupResolverError;

    fn rebuild_required(y: &GroupState<ID, OP, ORD>, message: &ORD::Message) -> bool {
        // Get all current tip operations.
        let heads = y
            .graph
            // TODO: clone required here when converting the GraphMap into a Graph. We do this
            // because the GraphMap api does not include the "externals" method, where as the
            // Graph api does. We use GraphMap as we can then access nodes by the id we assign
            // them rather than the internally assigned id generated when using Graph. We can use
            // Graph and track the indexes ourselves in order to avoid this conversion, or maybe
            // there is a way to get "externals" on GraphMap (which I didn't find yet). More
            // investigation required.
            .clone()
            .into_graph::<usize>()
            .externals(petgraph::Direction::Incoming)
            .map(|idx| y.graph.from_index(idx.index()))
            .collect::<Vec<_>>();

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
