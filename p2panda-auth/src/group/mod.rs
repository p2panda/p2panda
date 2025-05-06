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
    RS: Resolver<GroupState<ID, OP, ORD, GS>, ORD::Message>,
    ORD: Ordering<ID, OP, GroupControlMessage<ID, OP>>,
    GS: GroupStore<ID, OP, ORD::Message>,
{
    #[error("error occurred applying state change action")]
    StateChangeError(#[from] GroupStateError),

    #[error("resolver error: {0}")]
    ResolverError(RS::Error),

    #[error("ordering error: {0}")]
    OrderingError(ORD::Error),

    #[error("state {0} not found in group {1}")]
    StateNotFound(OP, ID),
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
        group_id: ID,
        member: GroupMember<ID>,
        access: Access,
    },
    Remove {
        group_id: ID,
        member: GroupMember<ID>,
    },
    Promote {
        group_id: ID,
        member: GroupMember<ID>,
        access: Access,
    },
    Demote {
        group_id: ID,
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

    pub fn group_id(&self) -> Option<ID> {
        let group_id = match self {
            GroupAction::Create { .. } => return None,
            GroupAction::Add { group_id, .. } => group_id,
            GroupAction::Remove { group_id, .. } => group_id,
            GroupAction::Promote { group_id, .. } => group_id,
            GroupAction::Demote { group_id, .. } => group_id,
        };
        Some(*group_id)
    }
}

/// Control messages which are processed by a group.
#[derive(Clone, Debug)]
pub enum GroupControlMessage<ID, OP> {
    Revoke { group_id: ID, id: OP },
    GroupAction { action: GroupAction<ID> },
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

    pub fn group_id(&self) -> Option<ID> {
        match self {
            GroupControlMessage::Revoke { group_id, .. } => Some(*group_id),
            GroupControlMessage::GroupAction { action } => action.group_id(),
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
    pub fn heads(&self) -> Vec<OP> {
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
            .externals(petgraph::Direction::Outgoing)
            .map(|idx| self.graph.from_index(idx.index()))
            .collect::<Vec<_>>()
    }

    fn transitive_heads<GS>(&self, group_store: &GS) -> Vec<OP>
    where
        GS: GroupStore<ID, OP, MSG>,
    {
        let mut transitive_heads = self.heads();
        for (member, ..) in self.members() {
            if let GroupMember::Group { id } = member {
                // All sub-groups should exist.
                let sub_group_inner = group_store.get(&id).unwrap();
                transitive_heads = vec![
                    transitive_heads,
                    sub_group_inner.transitive_heads(group_store),
                ]
                .concat();
            }
        }

        transitive_heads
    }

    fn current_state(&self) -> GroupMembersState<GroupMember<ID>> {
        let mut current_state = GroupMembersState::default();
        for state in self.heads() {
            // Unwrap as all "head" states should exist.
            let state = self.states.get(&state).unwrap();

            // Unwrap as no state merges should error.
            current_state = group_state::merge(state.clone(), current_state);
        }
        current_state
    }

    fn state_at(&self, operations: &Vec<OP>) -> GroupMembersState<GroupMember<ID>> {
        let mut y = GroupMembersState::default();
        for id in operations {
            let Some(previous_y) = self.states.get(id) else {
                // TODO: as dependencies contain _all_ dependencies, not only the "previous"
                // states from this group, then we have to just ignore not found states here for
                // now. Need to consider the best way to separate "dependencies" from "previous"
                // operations.
                continue;
                // return Err(GroupError::StateNotFound(*id, self.group_id));
            };
            y = group_state::merge(previous_y.clone(), y);
        }

        y
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

    pub fn transitive_members<GS>(&self, group_store: &GS) -> Vec<(ID, Access)>
    where
        GS: GroupStore<ID, OP, MSG>,
    {
        let mut members: HashMap<ID, Access> = HashMap::new();
        for (member, root_access) in self.members() {
            match member {
                GroupMember::Individual(id) => {
                    members.insert(id, root_access);
                }
                GroupMember::Group { id } => {
                    // Unwrap as all known sub groups should exist.
                    let sub_group = group_store.get(&id).unwrap();
                    let transitive_members = sub_group.transitive_members(group_store);
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
        members.into_iter().collect()
    }

    pub fn transitive_sub_groups<GS>(&self, group_store: &GS) -> Vec<(ID, Access)>
    where
        GS: GroupStore<ID, OP, MSG>,
    {
        let mut sub_groups: Vec<(ID, Access)> = Vec::new();
        for (member, access) in self.members() {
            if let GroupMember::Group { id } = member {
                // Unwrap as all known sub groups should exist.
                let sub_group = group_store.get(&id).unwrap();
                let transitive_sub_groups = sub_group.transitive_sub_groups(group_store);
                sub_groups = vec![transitive_sub_groups, sub_groups, vec![(id, access)]].concat();
            }
        }
        sub_groups.into_iter().collect()
    }
}

/// The internal state of a group.
///
/// TODO: We want to be able to serialize and deserialize group state, but this doesn't play well
/// with "shared state" like the group store abstraction. In this state object the "inner" state
/// can be serialized and deserialized.
#[derive(Clone, Debug)]
pub struct GroupState<ID, OP, ORD, GS>
where
    ID: IdentityHandle,
    OP: OperationId + Ord,
    ORD: Ordering<ID, OP, GroupControlMessage<ID, OP>>,
    GS: GroupStore<ID, OP, ORD::Message>,
{
    // ID of the local actor.
    pub my_id: ID,

    // The inner group state.
    pub inner: GroupStateInner<ID, OP, ORD::Message>,

    /// All groups known to this instance.
    pub group_store: GS,

    /// State for the orderer.
    pub orderer_state: ORD::State,
}

impl<ID, OP, ORD, GS> GroupState<ID, OP, ORD, GS>
where
    ID: IdentityHandle,
    OP: OperationId + Ord,
    ORD: Ordering<ID, OP, GroupControlMessage<ID, OP>>,
    GS: GroupStore<ID, OP, ORD::Message>,
{
    fn new(my_id: ID, group_id: ID, group_store: GS, orderer_state: ORD::State) -> Self {
        Self {
            my_id,
            inner: GroupStateInner {
                group_id,
                states: Default::default(),
                operations: Default::default(),
                ignore: Default::default(),
                graph: Default::default(),
            },
            group_store,
            orderer_state,
            // _phantom: PhantomData,
        }
    }

    pub fn heads(&self) -> Vec<OP> {
        self.inner.heads()
    }

    fn transitive_heads(&self) -> Vec<OP> {
        self.inner.transitive_heads(&self.group_store)
    }

    fn current_state(&self) -> GroupMembersState<GroupMember<ID>> {
        self.inner.current_state()
    }

    fn state_at<RS>(&self, operations: &Vec<OP>) -> GroupMembersState<GroupMember<ID>> {
        self.inner.state_at(operations)
    }

    pub fn members(&self) -> Vec<(GroupMember<ID>, Access)> {
        self.inner.members()
    }

    pub fn transitive_members(&self) -> Vec<(ID, Access)> {
        self.inner.transitive_members(&self.group_store)
    }

    pub fn transitive_sub_groups(&self) -> Vec<(ID, Access)> {
        self.inner.transitive_sub_groups(&self.group_store)
    }
}

#[derive(Clone, Debug, Default)]
pub struct Group<ID, OP, RS, ORD, GS> {
    _phantom: PhantomData<(ID, OP, RS, ORD, GS)>,
}

impl<ID, OP, RS, ORD, GS> AuthGraph<ID, OP, RS, ORD> for Group<ID, OP, RS, ORD, GS>
where
    ID: IdentityHandle + Display,
    OP: OperationId + Display + Ord,
    RS: Resolver<GroupState<ID, OP, ORD, GS>, ORD::Message> + Clone + Debug,
    ORD: Clone + Debug + Ordering<ID, OP, GroupControlMessage<ID, OP>>,
    GS: Clone + Debug + GroupStore<ID, OP, ORD::Message>,
{
    type State = GroupState<ID, OP, ORD, GS>;
    type Action = GroupControlMessage<ID, OP>;
    type Error = GroupError<ID, OP, RS, ORD, GS>;

    fn prepare(
        mut y: Self::State,
        operation: &Self::Action,
    ) -> Result<(GroupState<ID, OP, ORD, GS>, ORD::Message), GroupError<ID, OP, RS, ORD, GS>> {
        let dependencies = y.transitive_heads();
        let ordering_y = y.orderer_state.clone();
        let (ordering_y, message) = match ORD::next_message(ordering_y, dependencies, &operation) {
            Ok(message) => message,
            Err(_) => panic!(),
        };

        // Queue the message in the orderer.
        let ordering_y =
            ORD::queue(ordering_y, &message).map_err(|error| GroupError::OrderingError(error))?;
        y.orderer_state = ordering_y;
        Ok((y, message))
    }

    fn process(
        mut y: Self::State,
        operation: &ORD::Message,
    ) -> Result<Self::State, GroupError<ID, OP, RS, ORD, GS>> {
        let id = operation.id();
        let actor: ID = operation.sender();
        let control_message = operation.payload();

        // Get the group id from the control message.
        let group_id = match control_message.group_id() {
            Some(id) => id,
            None => {
                // Sanity check: operations without group id must be create.
                assert!(control_message.is_create());

                // The group takes the id of the sender (signing actor).
                actor
            }
        };

        if y.inner.group_id != group_id && control_message.is_create() {
            {
                let mut group_y = GroupState::new(
                    y.my_id,
                    group_id,
                    y.group_store.clone(),
                    y.orderer_state.clone(),
                );
                group_y = Self::process(group_y, operation)?;
                group_y.group_store.insert(&group_id, group_y.inner);
            }
            // Return the new group state.
            return Ok(y);
        }

        // If the group id is _not_ equal to the current group id then it must be from a
        // (possibly transitive) sub-group. Now we should recurse into all sub-groups trying to
        // find exactly where this operation should be processed.
        if y.inner.group_id != group_id {
            for (sub_group_id, _) in y.transitive_sub_groups() {
                if sub_group_id == group_id {
                    let inner = y.group_store.remove(&sub_group_id).unwrap();
                    let sub_group_y = GroupState {
                        my_id: y.my_id,
                        inner,
                        group_store: y.group_store.clone(),
                        orderer_state: y.orderer_state.clone(),
                    };
                    let sub_group_y_i = Self::process(sub_group_y, operation)?;
                    y.group_store.insert(&sub_group_id, sub_group_y_i.inner);
                }
            }

            // Return the new group state.
            return Ok(y);
        }

        // The operation concerns this group, so we can actually process it now.

        // The resolver implementation contains the logic which determines when rebuilds are
        // required, likely due to concurrent operations arriving which should trigger a new filter
        // to be constructed.
        if RS::rebuild_required(&y, &operation) {
            return Self::add_with_rebuild(y, operation.clone());
        }

        // Compute the members state by applying the new operation to it's claimed "previous"
        // state.
        //
        // This method validates that the actor has permission perform the action.
        match control_message {
            GroupControlMessage::GroupAction { action } => {
                y = Self::apply_action(
                    y,
                    group_id,
                    id,
                    GroupMember::Individual(actor),
                    operation.dependencies(),
                    action,
                )?;
            }
            // No action required as revokes were already processed when we resolved a filter.
            GroupControlMessage::Revoke { .. } => (),
        }

        // In all cases we add the new operation to the group states' graph and operations map.
        y.inner.graph.add_node(id);
        for previous in operation.dependencies() {
            y.inner.graph.add_edge(*previous, id, ());
        }
        y.inner.operations.push(operation.clone());

        Ok(y)
    }
}

impl<ID, OP, RS, ORD, GS> Group<ID, OP, RS, ORD, GS>
where
    ID: IdentityHandle,
    OP: OperationId + Ord,
    RS: Resolver<GroupState<ID, OP, ORD, GS>, ORD::Message> + Clone + Debug,
    ORD: Ordering<ID, OP, GroupControlMessage<ID, OP>> + Clone + Debug,
    GS: Clone + Debug + GroupStore<ID, OP, ORD::Message>,
{
    fn apply_action(
        mut y: GroupState<ID, OP, ORD, GS>,
        group_id: ID,
        id: OP,
        actor: GroupMember<ID>,
        previous: &Vec<OP>,
        action: &GroupAction<ID>,
    ) -> Result<GroupState<ID, OP, ORD, GS>, GroupError<ID, OP, RS, ORD, GS>> {
        // Sanity check, we should never call this method on the incorrect group.
        assert_eq!(y.inner.group_id, group_id);

        // Compute the members state by applying the new operation to it's claimed "previous"
        // state.
        let members_y = if previous.is_empty() {
            GroupMembersState::default()
        } else {
            y.state_at::<RS>(previous)
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

    fn add_with_rebuild(
        mut y: GroupState<ID, OP, ORD, GS>,
        operation: ORD::Message,
    ) -> Result<GroupState<ID, OP, ORD, GS>, GroupError<ID, OP, RS, ORD, GS>> {
        // Add all new operations to the graph and operations vec.
        y.inner.graph.add_node(operation.id());
        for previous in operation.dependencies() {
            y.inner.graph.add_edge(*previous, operation.id(), ());
        }
        y.inner.operations.push(operation);

        // Use the resolver to construct a filter for this group membership graph.
        y = RS::process(y).map_err(|error| GroupError::ResolverError(error))?;

        let mut y_i = GroupState::new(
            y.my_id,
            y.inner.group_id,
            y.group_store.clone(),
            y.orderer_state.clone(),
        );
        y_i.inner.ignore = y.inner.ignore;
        y_i.inner.graph = y.inner.graph;

        let mut create_found = false;
        for operation in y.inner.operations {
            let id = operation.id();
            let control_message = operation.payload();
            let actor = operation.sender();
            let dependencies = operation.dependencies();

            // Get the group id from the control message.
            let group_id = match control_message.group_id() {
                Some(id) => id,
                None => {
                    // Sanity check: operations without group id must be create.
                    assert!(control_message.is_create());

                    // The group takes the id of the sender (signing actor).
                    actor
                }
            };

            // Sanity check: we should only apply operations for this group.
            assert_eq!(y.inner.group_id, group_id);

            // Sanity check: the first operation must be a create.
            assert!(!create_found && !control_message.is_create());
            create_found = true;

            y_i = match control_message {
                GroupControlMessage::GroupAction { action } => Self::apply_action(
                    y_i,
                    group_id,
                    id,
                    GroupMember::Individual(actor),
                    dependencies,
                    action,
                )?,
                // No action required as revokes were already processed when we resolved a filter.
                GroupControlMessage::Revoke { .. } => y_i,
            };

            // Push the operation into the new states' operation vec.
            y_i.inner.operations.push(operation);
        }

        Ok(y_i)
    }
}
