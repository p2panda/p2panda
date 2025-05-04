use std::collections::{HashMap, HashSet};
use std::marker::PhantomData;

use group_state::MemberState;
use petgraph::algo::toposort;
use petgraph::prelude::DiGraphMap;
use petgraph::visit::NodeIndexable;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::group::access::Access;
use crate::group::group_state::GroupMembersState;
use crate::traits::{AuthGraph, IdentityHandle, Operation, OperationId, Ordering, Resolver};

mod access;
mod group_state;
mod resolver;

// TODO: introduce all error types.
#[derive(Debug, Error)]
pub enum GroupError {}

/// Actions which can be performed by group members.
#[derive(Clone, Debug, PartialEq)]
pub enum GroupAction<ID> {
    Create { initial_members: Vec<(ID, Access)> },
    Add { member: ID, access: Access },
    Remove { member: ID },
    Promote { member: ID, access: Access },
    Demote { member: ID, access: Access },
}

impl<ID> GroupAction<ID> {
    pub fn is_create(&self) -> bool {
        if let GroupAction::Create { .. } = self {
            true
        } else {
            false
        }
    }
}

/// Control messages which are processed by a group.
#[derive(Clone, Debug)]
pub enum GroupControlMessage<ID, OP> {
    Revoke { id: OP },
    GroupAction(GroupAction<ID>),
}

/// The internal state of a group.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GroupState<ID, OP, ORD>
where
    ID: IdentityHandle,
    OP: OperationId + Ord,
    ORD: Ordering<ID, OP, GroupControlMessage<ID, OP>>,
{
    // ID of the local actor.
    pub my_id: ID,

    /// States at every position in the message graph.
    pub states: HashMap<OP, GroupMembersState<ID>>,

    /// All operations processed by this group.
    pub operations: HashMap<OP, ORD::Message>,

    /// All operations who's actions should be ignored.
    pub ignore: HashSet<OP>,

    /// Operation graph.
    pub graph: DiGraphMap<OP, ()>,

    /// State for the orderer.
    pub orderer_state: ORD::State,
}

impl<ID, OP, ORD> GroupState<ID, OP, ORD>
where
    ID: IdentityHandle,
    OP: OperationId + Ord,
    ORD: Ordering<ID, OP, GroupControlMessage<ID, OP>>,
{
    fn heads(&self) -> Vec<OP> {
        self.graph
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
            .map(|idx| self.graph.from_index(idx.index()))
            .collect::<Vec<_>>()
    }

    fn current_state(&self) -> GroupMembersState<ID> {
        let mut current_state = GroupMembersState::default();
        for state in self.heads() {
            let state = self.states.get(&state).unwrap();
            current_state = group_state::merge(state.clone(), current_state).unwrap();
        }
        current_state
    }

    fn state_at(&self, operations: &Vec<OP>) -> GroupMembersState<ID> {
        let states: Vec<_> = operations
            .iter()
            .map(|id| self.states.get(id).unwrap()) // TODO: Error here.
            .cloned()
            .collect();

        // Merge all "previous states" into one.
        let mut y = GroupMembersState::default();
        for previous_y in states {
            // TODO: Decide what to do with errors here.
            y = group_state::merge(previous_y, y).unwrap();
        }

        y
    }

    pub fn members(&self) -> Vec<(ID, Access)> {
        self.current_state()
            .members
            .values()
            .filter_map(|state| {
                if state.is_member() {
                    Some((state.member.clone(), state.access))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
    }
}

#[derive(Clone, Debug, Default)]
pub struct Group<ID, OP, RS, ORD> {
    _phantom: PhantomData<(ID, OP, RS, ORD)>,
}

impl<ID, OP, RS, ORD> AuthGraph<ID, OP, RS, ORD> for Group<ID, OP, RS, ORD>
where
    ID: IdentityHandle + Serialize + for<'a> Deserialize<'a>,
    OP: OperationId + Ord + Serialize + for<'a> Deserialize<'a>,
    RS: Clone + Resolver<GroupState<ID, OP, ORD>, ORD::Message>,
    ORD: Clone + std::fmt::Debug + Ordering<ID, OP, GroupControlMessage<ID, OP>>,
{
    type State = GroupState<ID, OP, ORD>;
    type Action = GroupControlMessage<ID, OP>;
    type Error = GroupError;

    fn prepare(
        mut y: Self::State,
        operation: Self::Action,
    ) -> Result<(GroupState<ID, OP, ORD>, ORD::Message), GroupError> {
        let ordering_y = y.orderer_state.clone();
        let (ordering_y, message) = match ORD::next_message(ordering_y, &operation) {
            Ok(message) => message,
            Err(_) => panic!(),
        };

        // Queue the message in the orderer.
        let ordering_y = ORD::queue(ordering_y, &message).unwrap();
        y.orderer_state = ordering_y;
        Ok((y, message))
    }

    fn process(mut y: Self::State, operation: ORD::Message) -> Result<Self::State, GroupError> {
        let id = operation.id();
        let actor: ID = operation.sender();
        let control_message = operation.payload();

        // The resolver implementation contains the logic which determines when rebuilds are
        // required, likely due to concurrent operations arriving which should trigger a new filter
        // to be constructed.
        if RS::rebuild_required(&y, &operation) {
            return Self::add_with_rebuild(y, vec![operation]);
        }

        // Compute the members state by applying the new operation to it's claimed "previous"
        // state.
        //
        // This method validates that the actor has permission perform the action.
        match control_message {
            GroupControlMessage::GroupAction(action) => {
                let previous = operation.dependencies();
                let members_y = if previous.is_empty() {
                    GroupMembersState::default()
                } else {
                    y.state_at(previous)
                };
                let members_y_i = match Self::compute_next_state(members_y.clone(), actor, action) {
                    Ok(states) => states,
                    Err(_) => panic!(), // Handle all errors here.
                };

                // Only add the resulting members state to the states map if the operation isn't
                // flagged to be ignored.
                if !y.ignore.contains(&id) {
                    y.states.insert(id, members_y_i);
                } else {
                    y.states.insert(id, members_y);
                }
            }
            // No action required as revokes were already processed when we resolved a filter.
            GroupControlMessage::Revoke { .. } => (),
        }

        // In all cases we add the new operation to the group states' graph and operations map.
        y.graph.add_node(id);
        for previous in operation.dependencies() {
            y.graph.add_edge(*previous, id, ());
        }
        y.operations.insert(id, operation);

        Ok(y)
    }
}

impl<ID, OP, RS, ORD> Group<ID, OP, RS, ORD>
where
    ID: IdentityHandle + Serialize + for<'a> Deserialize<'a>,
    OP: OperationId + Ord + Serialize + for<'a> Deserialize<'a>,
    RS: Clone + Resolver<GroupState<ID, OP, ORD>, ORD::Message>,
    ORD: Clone + std::fmt::Debug + Ordering<ID, OP, GroupControlMessage<ID, OP>>,
{
    /// Compute the members state which results from applying the passed action.
    ///
    /// Validation that the actor performing the action has the required access level is performed
    /// internally.
    fn compute_next_state(
        y: GroupMembersState<ID>,
        actor: ID,
        action: &GroupAction<ID>,
    ) -> Result<GroupMembersState<ID>, GroupError> {
        // Apply the action to the merged states.
        let y_i = match action.clone() {
            GroupAction::Add { member, access } => {
                group_state::add_member(y, actor, member, access)
            }
            GroupAction::Remove { member } => group_state::remove_member(y, actor, member),
            GroupAction::Promote { member, access } => {
                group_state::promote(y, actor, member, access)
            }
            GroupAction::Demote { member, access } => {
                group_state::promote(y, actor, member, access)
            }
            GroupAction::Create { initial_members } => {
                let members = initial_members
                    .iter()
                    .map(|(member, access)| {
                        (
                            *member,
                            MemberState {
                                member: *member,
                                member_counter: 1,
                                access: *access,
                                access_counter: 0,
                            },
                        )
                    })
                    .collect::<HashMap<_, _>>();
                Ok(GroupMembersState { members })
            }
        }
        .unwrap(); // Handle these errors.

        Ok(y_i)
    }

    fn apply_action(
        mut y: GroupState<ID, OP, ORD>,
        id: OP,
        actor: ID,
        previous: &Vec<OP>,
        action: &GroupAction<ID>,
    ) -> Result<GroupState<ID, OP, ORD>, GroupError> {
        // Compute the members state by applying the new operation to it's claimed "previous"
        // state.
        //
        // This method validates that the actor has permission perform the action.
        let members_y = if previous.is_empty() {
            GroupMembersState::default()
        } else {
            y.state_at(previous)
        };
        let members_y_i = match Self::compute_next_state(members_y.clone(), actor, action) {
            Ok(states) => states,
            Err(_) => panic!(), // Handle all errors here.
        };

        // Only add the resulting members state to the states map if the operation isn't
        // flagged to be ignored.
        if !y.ignore.contains(&id) {
            y.states.insert(id, members_y_i);
        } else {
            y.states.insert(id, members_y);
        }

        Ok(y)
    }

    fn add_with_rebuild(
        mut y: GroupState<ID, OP, ORD>,
        operations: Vec<ORD::Message>,
    ) -> Result<GroupState<ID, OP, ORD>, GroupError> {
        // Add all new operations to the graph.
        for operation in operations {
            y.graph.add_node(operation.id());
            y.operations.insert(operation.id(), operation);
        }
        for (id, operation) in &y.operations {
            for previous in operation.dependencies() {
                // We know all nodes were added to the graph so we can unwrap here.
                y.graph.add_edge(*previous, *id, ());
            }
        }

        // Use the resolver to construct a filter for this group membership graph.
        let mut y = match RS::process(y) {
            Ok(result) => result,
            Err(_) => todo!(),
        };

        // Iterate over topologically sorted operations and compute state at each step.
        //
        // NOTE: we could specify that operations must already be provided in partial order, then
        // sorting again here wouldn't be required.
        let operations = match toposort(&y.graph, None) {
            Ok(operations) => operations,
            Err(_) => todo!(),
        };

        let mut create_found = false;
        for id in operations {
            let Some(operation) = y.operations.get(&id).cloned() else {
                panic!()
            };
            let control_message = operation.payload();
            let actor = operation.sender();

            if !create_found {
                // Check the first operation is a create.
                create_found = true;
            }

            y = match control_message {
                GroupControlMessage::GroupAction(action) => {
                    Self::apply_action(y, id, actor, operation.dependencies(), action)?
                }
                // No action required as revokes were already processed when we resolved a filter.
                GroupControlMessage::Revoke { .. } => y,
            }
        }

        Ok(y)
    }
}
