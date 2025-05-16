use std::collections::{HashMap, HashSet};
use std::fmt::{Debug, Display};
use std::marker::PhantomData;

use group_state::{GroupStateError, MemberState};
use petgraph::prelude::DiGraphMap;
use petgraph::visit::NodeIndexable;
use thiserror::Error;

use crate::group::access::Access;
use crate::group::group_state::GroupMembersState;
use crate::traits::{
    AuthGraph, GroupStore, IdentityHandle, Operation, OperationId, Ordering, Resolver,
};

mod access;
mod group_state;
mod resolver;
#[cfg(test)]
mod test_utils;
#[cfg(test)]
mod tests;

// TODO: introduce all error types.
#[derive(Debug, Error)]
pub enum GroupError<ID, OP, RS, ORD, GS>
where
    ID: IdentityHandle,
    OP: OperationId + Ord,
    RS: Resolver<GroupState<ID, OP, RS, ORD, GS>, ORD::Message>,
    ORD: Ordering<ID, OP, GroupControlMessage<ID, OP>>,
    GS: GroupStore<ID, GroupStateInner<ID, OP, ORD::Message>>,
{
    #[error("error occurred applying state change action")]
    StateChangeError(#[from] GroupStateError),

    #[error("expected sub-group {0} to exist in the store")]
    MissingSubGroup(ID),

    #[error("resolver error: {0}")]
    ResolverError(RS::Error),

    #[error("ordering error: {0}")]
    OrderingError(ORD::Error),

    #[error("group store error: {0}")]
    GroupStoreError(GS::Error),

    #[error("state {0} not found in group {1}")]
    StateNotFound(OP, ID),

    #[error("operation for group {0} processed in group {1}")]
    IncorrectGroupId(ID, ID),
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
pub enum GroupMember<ID> {
    Individual(ID),
    Group { id: ID },
}

impl<ID> IdentityHandle for GroupMember<ID> where ID: IdentityHandle {}

/// Actions which can be performed by group members.
#[derive(Clone, Debug, PartialEq)]
pub enum GroupAction<ID> {
    Create {
        initial_members: Vec<(GroupMember<ID>, Access)>,
    },
    Add {
        member: GroupMember<ID>,
        access: Access,
    },
    Remove {
        member: GroupMember<ID>,
    },
    Promote {
        member: GroupMember<ID>,
        access: Access,
    },
    Demote {
        member: GroupMember<ID>,
        access: Access,
    },
}

impl<ID> GroupAction<ID>
where
    ID: Copy,
{
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
    Revoke {
        group_id: ID,
        id: OP,
    },
    GroupAction {
        group_id: ID,
        action: GroupAction<ID>,
    },
}

impl<ID, OP> GroupControlMessage<ID, OP>
where
    ID: Copy,
{
    pub fn is_create(&self) -> bool {
        if let GroupControlMessage::GroupAction {
            action: GroupAction::Create { .. },
            ..
        } = self
        {
            true
        } else {
            false
        }
    }

    pub fn group_id(&self) -> ID {
        match self {
            GroupControlMessage::Revoke { group_id, .. } => *group_id,
            GroupControlMessage::GroupAction { group_id, .. } => *group_id,
        }
    }
}

#[derive(Clone, Debug)]
pub struct GroupStateInner<ID, OP, MSG>
where
    ID: IdentityHandle,
    OP: OperationId + Ord,
    MSG: Clone,
{
    // ID of the group.
    pub group_id: ID,

    /// States at every position in the operation graph.
    pub states: HashMap<OP, GroupMembersState<GroupMember<ID>>>,

    /// All operations processed by this group.
    ///
    /// Operations _must_ be kept in their partial-order (the order in which they were processed).
    pub operations: Vec<MSG>,

    /// All operations who's actions should be ignored.
    pub ignore: HashSet<OP>,

    /// Operation graph.
    pub graph: DiGraphMap<OP, ()>,
}

impl<ID, OP, MSG> GroupStateInner<ID, OP, MSG>
where
    ID: IdentityHandle,
    OP: OperationId + Ord,
    MSG: Clone,
{
    pub fn new(group_id: ID) -> Self {
        GroupStateInner {
            group_id,
            states: Default::default(),
            operations: Default::default(),
            ignore: Default::default(),
            graph: Default::default(),
        }
    }
}

/// The internal state of a group.
///
/// TODO: We want to be able to serialize and deserialize group state, but this doesn't play well
/// with "shared state" like the group store abstraction. In this state object the "inner" state
/// can be serialized and deserialized.
#[derive(Clone, Debug)]
pub struct GroupState<ID, OP, RS, ORD, GS>
where
    ID: IdentityHandle,
    OP: OperationId + Ord,
    RS: Resolver<Self, ORD::Message>,
    ORD: Ordering<ID, OP, GroupControlMessage<ID, OP>>,
    GS: GroupStore<ID, GroupStateInner<ID, OP, ORD::Message>>,
{
    // ID of the local actor.
    pub my_id: ID,

    // The inner group state.
    pub inner: GroupStateInner<ID, OP, ORD::Message>,

    /// All groups known to this instance.
    pub group_store_y: GS::State,

    /// State for the orderer.
    pub orderer_y: ORD::State,

    _phantom: PhantomData<RS>,
}

impl<ID, OP, RS, ORD, GS> GroupState<ID, OP, RS, ORD, GS>
where
    ID: IdentityHandle + Display,
    OP: OperationId + Display + Ord,
    RS: Resolver<GroupState<ID, OP, RS, ORD, GS>, ORD::Message> + Clone + Debug,
    ORD: Clone + Debug + Ordering<ID, OP, GroupControlMessage<ID, OP>>,
    GS: Clone + Debug + GroupStore<ID, GroupStateInner<ID, OP, ORD::Message>>,
{
    fn new(my_id: ID, group_id: ID, group_store_y: GS::State, orderer_y: ORD::State) -> Self {
        Self {
            my_id,
            inner: GroupStateInner {
                group_id,
                states: Default::default(),
                operations: Default::default(),
                ignore: Default::default(),
                graph: Default::default(),
            },
            group_store_y,
            orderer_y,
            _phantom: PhantomData,
        }
    }

    pub fn id(&self) -> ID {
        self.inner.group_id
    }

    fn new_from_inner(&self, inner: GroupStateInner<ID, OP, ORD::Message>) -> Self {
        let mut state = self.clone();
        state.inner = inner;
        state
    }

    pub fn heads(&self) -> Vec<OP> {
        self.inner
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
            .externals(petgraph::Direction::Outgoing)
            .map(|idx| self.inner.graph.from_index(idx.index()))
            .collect::<Vec<_>>()
    }

    fn transitive_heads(&self) -> Result<Vec<OP>, GroupError<ID, OP, RS, ORD, GS>> {
        let mut transitive_heads = self.heads();
        for (member, ..) in self.members() {
            if let GroupMember::Group { id } = member {
                let sub_group = self.get_sub_group(id)?;
                transitive_heads = vec![transitive_heads, sub_group.transitive_heads()?].concat();
            }
        }

        Ok(transitive_heads)
    }

    fn current_state(&self) -> GroupMembersState<GroupMember<ID>> {
        let mut current_state = GroupMembersState::default();
        for state in self.heads() {
            // Unwrap as all "head" states should exist.
            let state = self.inner.states.get(&state).unwrap();
            current_state = group_state::merge(state.clone(), current_state);
        }
        current_state
    }

    fn state_at(
        &self,
        operations: &Vec<OP>,
    ) -> Result<GroupMembersState<GroupMember<ID>>, GroupError<ID, OP, RS, ORD, GS>> {
        let mut y = GroupMembersState::default();
        for id in operations {
            let Some(previous_y) = self.inner.states.get(id) else {
                // We might be in a sub-group here processing dependencies which don't exist in
                // this graph, in that case we just ignore missing states.
                continue;
            };
            y = group_state::merge(previous_y.clone(), y);
        }

        Ok(y)
    }

    fn members_at(
        &self,
        operations: &Vec<OP>,
    ) -> Result<Vec<(GroupMember<ID>, Access)>, GroupError<ID, OP, RS, ORD, GS>> {
        let y = self.state_at(operations)?;
        Ok(y.members
            .values()
            .filter_map(|state| {
                if state.is_member() {
                    Some((state.member.clone(), state.access))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>())
    }

    fn transitive_members_at(
        &self,
        operations: &Vec<OP>,
    ) -> Result<Vec<(ID, Access)>, GroupError<ID, OP, RS, ORD, GS>> {
        let mut members: HashMap<ID, Access> = HashMap::new();
        for (member, root_access) in self.members_at(operations)? {
            match member {
                GroupMember::Individual(id) => {
                    members.insert(id, root_access);
                }
                GroupMember::Group { id } => {
                    let sub_group = self.get_sub_group(id)?;
                    let transitive_members = sub_group.transitive_members_at(operations)?;
                    for (transitive_member, transitive_access) in transitive_members {
                        members
                            .entry(transitive_member)
                            .and_modify(|access| {
                                if transitive_access > *access && transitive_access <= root_access {
                                    *access = transitive_access
                                }
                            })
                            .or_insert_with(|| {
                                if transitive_access <= root_access {
                                    transitive_access
                                } else {
                                    root_access
                                }
                            });
                    }
                }
            }
        }
        Ok(members.into_iter().collect())
    }

    pub fn members(&self) -> Vec<(GroupMember<ID>, Access)> {
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

    pub fn transitive_members(&self) -> Result<Vec<(ID, Access)>, GroupError<ID, OP, RS, ORD, GS>> {
        let heads = self.transitive_heads()?;
        let members = self.transitive_members_at(&heads)?;
        Ok(members)
    }

    pub fn transitive_sub_groups(
        &self,
    ) -> Result<Vec<(ID, Access)>, GroupError<ID, OP, RS, ORD, GS>> {
        let mut sub_groups: Vec<(ID, Access)> = Vec::new();
        for (member, access) in self.members() {
            if let GroupMember::Group { id } = member {
                let sub_group = self.get_sub_group(id)?;
                let transitive_sub_groups = sub_group.transitive_sub_groups()?;
                sub_groups = vec![transitive_sub_groups, sub_groups, vec![(id, access)]].concat();
            }
        }
        Ok(sub_groups.into_iter().collect())
    }

    pub fn get_sub_group(
        &self,
        id: ID,
    ) -> Result<GroupState<ID, OP, RS, ORD, GS>, GroupError<ID, OP, RS, ORD, GS>> {
        let inner = GS::get(&self.group_store_y, &id)
            .map_err(|error| GroupError::GroupStoreError(error))?;

        // We expect that groups are created and correctly present in the store before we process
        // any messages requiring us to query them, so this error can only occur if there is an
        // error in any higher orchestration system.
        let Some(inner) = inner else {
            return Err(GroupError::MissingSubGroup(id));
        };

        Ok(self.new_from_inner(inner))
    }
}

#[derive(Clone, Debug, Default)]
pub struct Group<ID, OP, RS, ORD, GS> {
    _phantom: PhantomData<(ID, OP, RS, ORD, GS)>,
}

// ORCHESTRATION REQUIREMENT NOTES:
// 1) when a sub-group receives an operation which effects the set of admin members, then any root
//    groups should be rebuilt in case the resolver needs to react to the admin change.
impl<ID, OP, RS, ORD, GS> AuthGraph<ID, OP, RS, ORD> for Group<ID, OP, RS, ORD, GS>
where
    ID: IdentityHandle + Display,
    OP: OperationId + Display + Ord,
    RS: Resolver<GroupState<ID, OP, RS, ORD, GS>, ORD::Message> + Clone + Debug,
    ORD: Clone + Debug + Ordering<ID, OP, GroupControlMessage<ID, OP>>,
    GS: Clone + Debug + GroupStore<ID, GroupStateInner<ID, OP, ORD::Message>>,
{
    type State = GroupState<ID, OP, RS, ORD, GS>;
    type Action = GroupControlMessage<ID, OP>;
    type Error = GroupError<ID, OP, RS, ORD, GS>;

    fn prepare(
        mut y: Self::State,
        operation: &Self::Action,
    ) -> Result<(GroupState<ID, OP, RS, ORD, GS>, ORD::Message), GroupError<ID, OP, RS, ORD, GS>>
    {
        let mut dependencies = y.transitive_heads()?;

        if let GroupControlMessage::GroupAction {
            action:
                GroupAction::Add {
                    member: GroupMember::Group { id },
                    ..
                },
            ..
        } = operation
        {
            let added_sub_group = y.get_sub_group(*id)?;
            dependencies.extend(&added_sub_group.transitive_heads()?);
        };

        if let GroupControlMessage::GroupAction {
            action: GroupAction::Create { initial_members },
            ..
        } = operation
        {
            for (member, _) in initial_members {
                if let GroupMember::Group { id } = member {
                    let sub_group = y.get_sub_group(*id)?;
                    dependencies.extend(&sub_group.transitive_heads()?);
                }
            }
        };

        let previous = y.heads();
        let ordering_y = y.orderer_y.clone();
        let (ordering_y, message) =
            match ORD::next_message(ordering_y, dependencies, previous, &operation) {
                Ok(message) => message,
                Err(_) => panic!(),
            };

        // Queue the message in the orderer.
        let ordering_y =
            ORD::queue(ordering_y, &message).map_err(|error| GroupError::OrderingError(error))?;
        y.orderer_y = ordering_y;
        Ok((y, message))
    }

    fn process(
        mut y: Self::State,
        operation: &ORD::Message,
    ) -> Result<Self::State, GroupError<ID, OP, RS, ORD, GS>> {
        let operation_id = operation.id();
        let actor = operation.sender();
        let control_message = operation.payload();
        let previous = operation.previous();
        let group_id = control_message.group_id();

        if y.inner.group_id != group_id {
            // This operation is not intended for this group.
            return Err(GroupError::IncorrectGroupId(group_id, y.inner.group_id));
        }

        // The resolver implementation contains the logic which determines when rebuilds are
        // required, likely due to concurrent operations arriving which should trigger a new filter
        // to be constructed.
        if RS::rebuild_required(&y, &operation) {
            // Add all new operations to the graph and operations vec.
            y.inner.graph.add_node(operation.id());
            for previous in operation.previous() {
                y.inner.graph.add_edge(*previous, operation.id(), ());
            }
            y.inner.operations.push(operation.clone());

            return Self::rebuild(&y);
        }

        // Compute the members state by applying the new operation to it's claimed "previous"
        // state.
        //
        // This method validates that the actor has permission perform the action.
        match control_message {
            GroupControlMessage::GroupAction { action, .. } => {
                y = Self::apply_action(
                    y,
                    operation_id,
                    GroupMember::Individual(actor),
                    previous,
                    action,
                )?;
            }
            // No action required as revokes were already processed when we resolved a filter.
            GroupControlMessage::Revoke { .. } => (),
        }

        // Add the new operation to the group states' graph and operations vec.
        y.inner.graph.add_node(operation_id);
        for previous in previous {
            y.inner.graph.add_edge(*previous, operation_id, ());
        }
        y.inner.operations.push(operation.clone());
        y.group_store_y = GS::insert(y.group_store_y, &group_id, &y.inner)
            .map_err(|error| GroupError::GroupStoreError(error))?;

        Ok(y)
    }
}

impl<ID, OP, RS, ORD, GS> Group<ID, OP, RS, ORD, GS>
where
    ID: IdentityHandle + Display,
    OP: OperationId + Display + Ord,
    RS: Resolver<GroupState<ID, OP, RS, ORD, GS>, ORD::Message> + Clone + Debug,
    ORD: Clone + Debug + Ordering<ID, OP, GroupControlMessage<ID, OP>>,
    GS: Clone + Debug + GroupStore<ID, GroupStateInner<ID, OP, ORD::Message>>,
{
    fn apply_action(
        mut y: GroupState<ID, OP, RS, ORD, GS>,
        id: OP,
        actor: GroupMember<ID>,
        previous: &Vec<OP>,
        action: &GroupAction<ID>,
    ) -> Result<GroupState<ID, OP, RS, ORD, GS>, GroupError<ID, OP, RS, ORD, GS>> {
        // Compute the members state by applying the new operation to it's claimed "previous"
        // state.
        let members_y = if previous.is_empty() {
            GroupMembersState::default()
        } else {
            y.state_at(previous)?
        };

        let members_y_copy = members_y.clone();
        let members_y_i = match action.clone() {
            GroupAction::Add { member, access, .. } => {
                group_state::add_member(members_y_copy, actor, member, access)
            }
            GroupAction::Remove { member, .. } => {
                group_state::remove_member(members_y_copy, actor, member)
            }
            GroupAction::Promote { member, access, .. } => {
                group_state::promote(members_y_copy, actor, member, access)
            }
            GroupAction::Demote { member, access, .. } => {
                group_state::demote(members_y_copy, actor, member, access)
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
        }?;

        // Only add the resulting members state to the states map if the operation isn't
        // flagged to be ignored.
        if !y.inner.ignore.contains(&id) {
            y.inner.states.insert(id, members_y_i);
        } else {
            y.inner.states.insert(id, members_y);
        }
        Ok(y)
    }

    fn rebuild(
        y: &GroupState<ID, OP, RS, ORD, GS>,
    ) -> Result<GroupState<ID, OP, RS, ORD, GS>, GroupError<ID, OP, RS, ORD, GS>> {
        // Use the resolver to construct a filter for this group membership graph.
        let y = RS::process(y.clone()).map_err(|error| GroupError::ResolverError(error))?;

        let mut y_i = GroupState::new(
            y.my_id,
            y.inner.group_id,
            y.group_store_y.clone(),
            y.orderer_y.clone(),
        );
        y_i.inner.ignore = y.inner.ignore;
        y_i.inner.graph = y.inner.graph;

        let mut create_found = false;
        for operation in y.inner.operations {
            let id = operation.id();
            let actor = operation.sender();
            let control_message = operation.payload();
            let group_id = control_message.group_id();
            let previous = operation.previous();

            // Sanity check: we should only apply operations for this group.
            assert_eq!(y.inner.group_id, group_id);

            // Sanity check: the first operation must be a create.
            assert!(!create_found && !control_message.is_create());
            create_found = true;

            y_i = match control_message {
                GroupControlMessage::GroupAction { action, .. } => {
                    Self::apply_action(y_i, id, GroupMember::Individual(actor), previous, action)?
                }
                // No action required as revokes were already processed when we resolved a filter.
                GroupControlMessage::Revoke { .. } => y_i,
            };

            // Push the operation into the new states' operation vec.
            y_i.inner.operations.push(operation);
        }

        Ok(y_i)
    }
}
