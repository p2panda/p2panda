// SPDX-License-Identifier: MIT OR Apache-2.0
// #![allow(clippy::type_complexity)]
// #![allow(dead_code)]

use std::collections::{HashMap, HashSet};
use std::fmt::{Debug, Display};
use std::marker::PhantomData;

use petgraph::prelude::DiGraphMap;
use petgraph::visit::NodeIndexable;
use thiserror::Error;

pub use crate::group::resolver::{GroupResolver, GroupResolverError};
pub use crate::group::state::{Access, GroupMembersState, GroupMembershipError, MemberState};
use crate::traits::{
    AuthGroup, GroupStore, IdentityHandle, Operation, OperationId, Ordering, Resolver,
};

#[cfg(any(test, feature = "test_utils"))]
mod display;
mod resolver;
mod state;
#[cfg(any(test, feature = "test_utils"))]
pub mod test_utils;
#[cfg(test)]
mod tests;

/// A group member which can be a single stateless individual, or a stateful group. In both cases
/// the member identifier is the same generic ID.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, PartialOrd, Ord)]
pub enum GroupMember<ID> {
    Individual(ID),
    Group(ID),
}

impl<ID> GroupMember<ID>
where
    ID: Copy,
{
    pub fn id(&self) -> ID {
        match self {
            GroupMember::Individual(id) => *id,
            GroupMember::Group { id, .. } => *id,
        }
    }
}

impl<ID> IdentityHandle for GroupMember<ID> where ID: IdentityHandle {}

/// Actions which can be applied to a group.
#[derive(Clone, Debug, PartialEq)]
pub enum GroupAction<ID, C> {
    Create {
        initial_members: Vec<(GroupMember<ID>, Access<C>)>,
    },
    Add {
        member: GroupMember<ID>,
        access: Access<C>,
    },
    Remove {
        member: GroupMember<ID>,
    },
    Promote {
        member: GroupMember<ID>,
        access: Access<C>,
    },
    Demote {
        member: GroupMember<ID>,
        access: Access<C>,
    },
}

impl<ID, C> GroupAction<ID, C>
where
    ID: Copy,
{
    /// Returns true if this is a create action.
    pub fn is_create(&self) -> bool {
        matches!(self, GroupAction::Create { .. })
    }
}

/// Control messages which are processed by a group.
///
/// There are two variants, one containing a group action and the id of the group where the action
/// should be applied. The other is a special message which can be used to "undo" a message which
/// has been applied to the group in the past.
#[derive(Clone, Debug)]
pub enum GroupControlMessage<ID, OP, C> {
    /// An action to apply to the group state.
    GroupAction {
        group_id: ID,
        action: GroupAction<ID, C>,
    },

    /// A revoke message can be published in order to explicitly invalidate other messages already
    /// included in a group graph. This action is agnostic to any, probably more nuanced,
    /// resolving logic which reacts to group actions.
    ///
    /// TODO: revoking messages is not implemented yet. I'm still considering if it is required in
    /// or initial group implementation or something that can come later. There are distinct
    /// benefits to revoking messages, over "just" making sure to resolve concurrent group action
    /// conflicts (for example with "strong removal") strategy. By issuing a revoke message
    /// revoking the message which first added a member into the group, it's possible to
    /// completely erase that member from the group history. There can be an implicit "seniority"
    /// rule in play, where it's only possible for an admin to revoke messages that they
    /// published, or from when they were in the group.
    Revoke { group_id: ID, id: OP },
}

impl<ID, OP, C> GroupControlMessage<ID, OP, C>
where
    ID: Copy,
{
    /// Returns true if this is a create control message.
    pub fn is_create(&self) -> bool {
        matches!(
            self,
            GroupControlMessage::GroupAction {
                action: GroupAction::Create { .. },
                ..
            }
        )
    }

    /// Id of the group this message should be applied to.
    pub fn group_id(&self) -> ID {
        match self {
            GroupControlMessage::Revoke { group_id, .. } => *group_id,
            GroupControlMessage::GroupAction { group_id, .. } => *group_id,
        }
    }
}

/// The state of a group, the local actor id, as well as state objects for the global
/// group store and orderer.
#[derive(Debug)]
#[cfg_attr(any(test, feature = "test_utils"), derive(Clone))]
pub struct GroupState<ID, OP, C, RS, ORD, GS>
where
    ID: IdentityHandle,
    OP: OperationId,
    ORD: Ordering<ID, OP, GroupControlMessage<ID, OP, C>>,
    GS: GroupStore<ID, OP, C, RS, ORD>,
{
    /// ID of the local actor.
    pub my_id: ID,

    /// ID of the group.
    pub group_id: ID,

    /// Group state at every position in the operation graph.
    pub states: HashMap<OP, GroupMembersState<GroupMember<ID>, C>>,

    /// All operations processed by this group.
    ///
    /// Operations _must_ be kept in their partial-order (the order in which they were processed).
    pub operations: Vec<ORD::Message>,

    /// All operations who's actions should be ignored.
    pub ignore: HashSet<OP>,

    /// Operation graph for this group.
    pub graph: DiGraphMap<OP, ()>,

    /// State for the orderer.
    pub orderer_y: ORD::State,

    /// All groups known to this instance.
    pub group_store: GS,

    _phantom: PhantomData<RS>,
}

impl<ID, OP, C, RS, ORD, GS> GroupState<ID, OP, C, RS, ORD, GS>
where
    ID: IdentityHandle,
    OP: OperationId + Ord,
    C: Clone + Debug + PartialEq + PartialOrd,
    RS: Resolver<ORD::Message>,
    ORD: Ordering<ID, OP, GroupControlMessage<ID, OP, C>>,
    GS: GroupStore<ID, OP, C, RS, ORD> + Debug,
{
    /// Instantiate a new group state.
    pub fn new(my_id: ID, group_id: ID, group_store: GS, orderer_y: ORD::State) -> Self {
        Self {
            my_id,
            group_id,
            states: Default::default(),
            operations: Default::default(),
            ignore: Default::default(),
            graph: Default::default(),
            group_store,
            orderer_y,
            _phantom: PhantomData,
        }
    }

    /// The id of this group.
    pub fn id(&self) -> ID {
        self.group_id
    }

    /// The current graph tips for this group.
    pub fn heads(&self) -> HashSet<OP> {
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
            .collect::<HashSet<_>>()
    }

    /// The current graph tips for this group and any sub-groups who are currently members.
    fn transitive_heads(&self) -> Result<HashSet<OP>, GroupError<ID, OP, C, RS, ORD, GS>> {
        let mut transitive_heads = self.heads();
        for (member, ..) in self.members() {
            if let GroupMember::Group(id) = member {
                let sub_group = self.get_sub_group(id)?;
                transitive_heads.extend(sub_group.transitive_heads()?);
            }
        }

        Ok(transitive_heads)
    }

    /// The current state of this group.
    ///
    /// This method gets the state at all graph tips and then merges them together into one new
    /// state which represents the current state of the group.
    pub fn current_state(&self) -> GroupMembersState<GroupMember<ID>, C> {
        let mut current_state = GroupMembersState::default();
        for state in self.heads() {
            // Unwrap as all "head" states should exist.
            let state = self.states.get(&state).unwrap();
            current_state = state::merge(state.clone(), current_state);
        }
        current_state
    }

    fn state_at_inner(
        &self,
        dependencies: &mut HashSet<OP>,
    ) -> Result<GroupMembersState<GroupMember<ID>, C>, GroupError<ID, OP, C, RS, ORD, GS>> {
        let mut y = GroupMembersState::default();
        let mut visited = HashSet::new();
        for id in dependencies.iter() {
            let Some(previous_y) = self.states.get(id) else {
                // We might be in a sub-group here processing dependencies which don't exist in
                // this graph, in that case we just ignore missing states.
                continue;
            };
            // Merge all dependency states from this group together.
            y = state::merge(previous_y.clone(), y);
            visited.insert(*id);
        }

        // remove all visited states from the dependencies set.
        for id in visited {
            dependencies.remove(&id);
        }

        Ok(y)
    }

    /// Get the state of a group at a certain point in it's history.
    pub fn state_at(
        &self,
        dependencies: &HashSet<OP>,
    ) -> Result<GroupMembersState<GroupMember<ID>, C>, GroupError<ID, OP, C, RS, ORD, GS>> {
        let mut dependencies = dependencies.clone();
        let state = self.state_at_inner(&mut dependencies)?;

        if !dependencies.is_empty() {
            return Err(GroupError::StatesNotFound(
                dependencies.into_iter().collect::<Vec<_>>(),
                self.id(),
            ));
        }

        Ok(state)
    }

    fn members_at_inner(
        &self,
        dependencies: &mut HashSet<OP>,
    ) -> Result<Vec<(GroupMember<ID>, Access<C>)>, GroupError<ID, OP, C, RS, ORD, GS>> {
        let y = self.state_at_inner(dependencies)?;
        let members = y
            .members
            .into_iter()
            .filter_map(|(id, state)| {
                if state.is_member() {
                    Some((id, state.access))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        Ok(members)
    }

    /// Get the group members at a certain point in groups history.
    pub fn members_at(
        &self,
        dependencies: &HashSet<OP>,
    ) -> Result<Vec<(GroupMember<ID>, Access<C>)>, GroupError<ID, OP, C, RS, ORD, GS>> {
        let mut dependencies = dependencies.clone();
        let members = self.members_at_inner(&mut dependencies)?;

        if !dependencies.is_empty() {
            return Err(GroupError::DependenciesNotFound(
                dependencies.into_iter().collect::<Vec<_>>(),
                self.id(),
            ));
        }

        Ok(members)
    }

    fn transitive_members_at_inner(
        &self,
        dependencies: &mut HashSet<OP>,
    ) -> Result<Vec<(ID, Access<C>)>, GroupError<ID, OP, C, RS, ORD, GS>> {
        let mut members: HashMap<ID, Access<C>> = HashMap::new();

        // Get members of a group at a certain point in the groups history.
        for (member, root_access) in self.members_at_inner(dependencies)? {
            match member {
                GroupMember::Individual(id) => {
                    // If this is an individual member, then add them straight to the members map.
                    members.insert(id, root_access.clone());
                }
                GroupMember::Group(id) => {
                    // If this is a sub-group member, then get the sub-group state from the store
                    // and recurse into the group passing the dependencies set which identify the
                    // particular states we're interested in.
                    let sub_group = self.get_sub_group(id)?;

                    // The access level for all transitive members must not be greater than the
                    // access level assigned to the current sub-group.
                    let transitive_members = sub_group.transitive_members_at_inner(dependencies)?;

                    // For each transitive member, add them to the members map if they were not
                    // already a member, assigning them the correct access level. If they were
                    // already a member, then modify their existing access level _if_ it elevates
                    // their access to a higher level, but not higher than the current sub.
                    for (transitive_member, transitive_access) in transitive_members {
                        let root_access_copy = root_access.clone();
                        members
                            .entry(transitive_member)
                            .and_modify(|access| {
                                // If the transitive access level this member holds (the access
                                // level the member has in it's sub-group) is greater than it's
                                // current access level, but not greater than the root access
                                // level (the access level initially assigned from the parent
                                // group) then update the access level.
                                if transitive_access > *access
                                    && transitive_access <= root_access_copy
                                {
                                    *access = transitive_access.clone()
                                }
                            })
                            .or_insert_with(|| {
                                if transitive_access <= root_access_copy {
                                    transitive_access
                                } else {
                                    root_access_copy
                                }
                            });
                    }
                }
            }
        }
        Ok(members.into_iter().collect())
    }

    /// Get all transitive members of the group at a certain point in it's history.
    ///
    /// This method recurses into all sub-groups collecting all "tip" members, which are the
    /// stateless "individual" members of a group, likely identified by a public key.
    pub fn transitive_members_at(
        &self,
        dependencies: &HashSet<OP>,
    ) -> Result<Vec<(ID, Access<C>)>, GroupError<ID, OP, C, RS, ORD, GS>> {
        let mut dependencies = dependencies.clone();
        let members = self.transitive_members_at_inner(&mut dependencies)?;

        if !dependencies.is_empty() {
            return Err(GroupError::DependenciesNotFound(
                dependencies.into_iter().collect::<Vec<_>>(),
                self.id(),
            ));
        }

        Ok(members)
    }

    // Get all current members of the group.
    pub fn members(&self) -> Vec<(GroupMember<ID>, Access<C>)> {
        self.current_state()
            .members
            .into_iter()
            .filter_map(|(id, state)| {
                if state.is_member() {
                    Some((id, state.access))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
    }

    /// Get all current transitive members of the group.
    ///
    /// This method recurses into all sub-groups collecting all "tip" members, which are the
    /// stateless "individual" members of a group, likely identified by a public key.
    pub fn transitive_members(
        &self,
    ) -> Result<Vec<(ID, Access<C>)>, GroupError<ID, OP, C, RS, ORD, GS>> {
        let heads = self.transitive_heads()?;
        let members = self.transitive_members_at(&heads)?;
        Ok(members)
    }

    /// Get a sub group from the group store.
    fn get_sub_group(
        &self,
        id: ID,
    ) -> Result<GroupState<ID, OP, C, RS, ORD, GS>, GroupError<ID, OP, C, RS, ORD, GS>> {
        let y = self
            .group_store
            .get(&id)
            .map_err(|error| GroupError::GroupStoreError(error))?;

        // We expect that groups are created and correctly present in the store before we process
        // any messages requiring us to query them, so this error can only occur if there is an
        // error in any higher orchestration system.
        let Some(y) = y else {
            return Err(GroupError::MissingSubGroup(id));
        };

        Ok(y)
    }
}

#[derive(Clone, Debug, Default)]
pub struct Group<ID, OP, C, RS, ORD, GS> {
    _phantom: PhantomData<(ID, OP, C, RS, ORD, GS)>,
}

/// Core auth protocol for maintaining group membership state in a distributed system. Group
/// members can be assigned different access levels, where only a sub-set of members can mutate
/// the state of the group itself.
///
/// The core data type is an Acyclic Directed Graph of `GroupControlMessage`s. Messages contain
/// group control messages which mutate the previous group state. Messages refer to the "previous"
/// state (set of graph tips) which the action they contain should be applied to, these references
/// make up the edges in the graph. Additionally, messages have a set of "dependencies" which
/// messages which could be part of any auth sub-group.
///
/// A requirement of the protocol is that all messages are processed in partial-order. When using
/// a dependency graph structure (as is the case in this implementation) it is possible to achieve
/// this by only processing a message once all it's dependencies have themselves been processed.
///
/// Group state is maintained using a state-based CRDT `GroupMembersState`. Every time a message
/// is processed, a new state is generated and added to the map of all states. When a new messages
/// is received, it's "previous" state is calculated and then the message applied, resulting in a
/// new state. This approach allows one to use the state-based CRDT `merge` method to combine
/// states from any points in the group history into a new state. This property is what allows us
/// to process messages in partial- rather than total-order.
///
/// Group membership rules are checked when an action is applied to the previous state, read more
/// in the `group_crdt` module.
///
/// This is an implementation of the `AuthGroup` trait which requires a `prepare` and `process`
/// method. This implementation allows for providing several generic parameters which allows for
/// integration into different systems and customization of how group change conflicts are
/// handled.
///
/// - Resolver (RS): contains logic for deciding when group state rebuilds are required, and how
///   concurrent actions are handled.
/// - Orderer (ORD): the orderer implements an approach to ordering messages, the protocol
///   requires that all messages are processed in partial-order, but exactly how this is achieved
///   is not specified.
/// - Group Store (GS): global store containing states for all known groups.
impl<ID, OP, C, RS, ORD, GS> AuthGroup<ID, OP, RS, ORD> for Group<ID, OP, C, RS, ORD, GS>
where
    ID: IdentityHandle + Display,
    OP: OperationId + Ord + Display,
    C: Clone + Debug + PartialEq + PartialOrd,
    RS: Resolver<ORD::Message, State = GroupState<ID, OP, C, RS, ORD, GS>> + Debug,
    ORD: Ordering<ID, OP, GroupControlMessage<ID, OP, C>> + Debug,
    ORD::Message: Clone,
    GS: GroupStore<ID, OP, C, RS, ORD> + Debug,
{
    type State = GroupState<ID, OP, C, RS, ORD, GS>;
    type Action = GroupControlMessage<ID, OP, C>;
    type Error = GroupError<ID, OP, C, RS, ORD, GS>;

    /// Prepare a next message/operation which should include all meta-data required for ordering
    /// auth group operations. An ORD implementation needs to guarantee that operations are
    /// processed after any dependencies they have on the group graph they are part of, as well as
    /// any sub-groups.
    ///
    /// The method `GroupState::heads` and `GroupState::transitive_heads` can be used to retrieve the
    /// operation ids of these operation dependencies.
    fn prepare(
        mut y: Self::State,
        action: &Self::Action,
    ) -> Result<
        (GroupState<ID, OP, C, RS, ORD, GS>, ORD::Message),
        GroupError<ID, OP, C, RS, ORD, GS>,
    > {
        // Get the next operation from our global orderer. The operation wraps the action we want
        // to perform, adding ordering and author meta-data.
        let ordering_y = y.orderer_y;
        let (ordering_y, operation) = match ORD::next_message(ordering_y, action) {
            Ok(operation) => operation,
            Err(_) => panic!(),
        };

        y.orderer_y = ordering_y;
        Ok((y, operation))
    }

    /// Process an operation created locally or received from a remote peer.
    fn process(
        mut y: Self::State,
        operation: &ORD::Message,
    ) -> Result<Self::State, GroupError<ID, OP, C, RS, ORD, GS>> {
        let operation_id = operation.id();
        let actor = operation.sender();
        let control_message = operation.payload();
        let previous_operations = HashSet::from_iter(operation.previous().clone());
        let group_id = control_message.group_id();

        // TODO: this is a bit of a sanity check, if we want to check for duplicate operation
        // processing here in the groups api then there should probably be a hashset of operations
        // ids maintained on the struct for efficient lookup.
        if y.operations.iter().any(|op| op.id() == operation_id) {
            return Err(GroupError::DuplicateOperation(operation_id, group_id));
        }

        if y.group_id != group_id {
            // This operation is not intended for this group.
            return Err(GroupError::IncorrectGroupId(group_id, y.group_id));
        }

        // The resolver implementation contains the logic which determines when rebuilds are
        // required.
        //
        // TODO: before performing this check we want to actually apply the operation to the
        // group. This will allow us to handle any validation which occur at that point already.
        if RS::rebuild_required(&y, operation) {
            // Add all new operations to the graph and operations vec.
            y.graph.add_node(operation.id());
            for previous in previous_operations {
                y.graph.add_edge(previous, operation.id(), ());
            }
            y.operations.push(operation.clone());

            // Perform the re-build and return the new state.
            return Self::rebuild(y);
        }

        // Compute the members state by applying the new operation to it's claimed "previous"
        // state.
        //
        // This method validates that the actor has permission to perform the action.
        match control_message {
            GroupControlMessage::GroupAction { action, .. } => {
                y = Self::apply_action(
                    y,
                    operation_id,
                    GroupMember::Individual(actor),
                    &previous_operations,
                    &action,
                )?;
            }
            // No action required as revokes would have triggered a rebuild in the previous step.
            //
            // TODO: we could bake in revoke support here if we want to keep it as a core feature
            // (on top of any provided Resolver).
            GroupControlMessage::Revoke { .. } => (),
        }

        // Add the new operation to the group states' graph and operations vec.
        y.graph.add_node(operation_id);
        for previous in previous_operations {
            y.graph.add_edge(previous, operation_id, ());
        }
        y.operations.push(operation.clone());

        // Update the group in the store.
        y.group_store
            .insert(&group_id, &y)
            .map_err(|error| GroupError::GroupStoreError(error))?;

        Ok(y)
    }
}

impl<ID, OP, C, RS, ORD, GS> Group<ID, OP, C, RS, ORD, GS>
where
    ID: IdentityHandle + Display,
    OP: OperationId + Ord + Display,
    C: Clone + Debug + PartialEq + PartialOrd,
    RS: Resolver<ORD::Message, State = GroupState<ID, OP, C, RS, ORD, GS>> + Debug,
    ORD: Ordering<ID, OP, GroupControlMessage<ID, OP, C>> + Debug,
    ORD::Message: Clone,
    GS: GroupStore<ID, OP, C, RS, ORD> + Debug,
{
    /// Apply an action to a single group state.
    fn apply_action(
        mut y: GroupState<ID, OP, C, RS, ORD, GS>,
        id: OP,
        actor: GroupMember<ID>,
        previous: &HashSet<OP>,
        action: &GroupAction<ID, C>,
    ) -> Result<GroupState<ID, OP, C, RS, ORD, GS>, GroupError<ID, OP, C, RS, ORD, GS>> {
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
                state::add(members_y_copy, actor, member, access)
            }
            GroupAction::Remove { member, .. } => state::remove(members_y_copy, actor, member),
            GroupAction::Promote { member, .. } => {
                // TODO: need changes in the group_crdt api so that we can pass in the access
                // level rather than only the conditions.
                state::promote(members_y_copy, actor, member, None)
            }
            GroupAction::Demote { member, .. } => {
                // TODO: need changes in the group_crdt api so that we can pass in the access
                // level rather than only the conditions.
                state::demote(members_y_copy, actor, member, None)
            }
            GroupAction::Create { initial_members } => Ok(state::create(&initial_members)),
        }
        .map_err(|error| GroupError::StateChangeError(error))?;

        // Only add the resulting members state to the states map if the operation isn't
        // flagged to be ignored.
        if !y.ignore.contains(&id) {
            y.states.insert(id, members_y_i);
        } else {
            y.states.insert(id, members_y);
        }
        Ok(y)
    }

    fn rebuild(
        y: GroupState<ID, OP, C, RS, ORD, GS>,
    ) -> Result<GroupState<ID, OP, C, RS, ORD, GS>, GroupError<ID, OP, C, RS, ORD, GS>> {
        // Process the group state with the provided resolver. This will populate the set of
        // messages which should be ignored when applying group control messages.
        let mut y_i = RS::process(y).map_err(|error| GroupError::ResolverError(error))?;

        let mut create_found = false;

        // Apply every operation.
        let operations = y_i.operations.clone();
        for operation in operations {
            let id = operation.id();
            let actor = operation.sender();
            let control_message = operation.payload();
            let group_id = control_message.group_id();
            let previous_operations = HashSet::from_iter(operation.previous().clone());

            // Sanity check: we should only apply operations for this group.
            assert_eq!(y_i.group_id, group_id);

            // Sanity check: the first operation must be a create and all other operations must not be.
            if create_found {
                assert!(!control_message.is_create())
            } else {
                assert!(control_message.is_create())
            }

            create_found = true;

            y_i = match control_message {
                GroupControlMessage::GroupAction { action, .. } => Self::apply_action(
                    y_i,
                    id,
                    GroupMember::Individual(actor),
                    &previous_operations,
                    &action,
                )?,
                // No action required as revokes were already processed and the `ignore` field populated.
                GroupControlMessage::Revoke { .. } => y_i,
            };

            // Push the operation into the new states' operation vec.
            y_i.operations.push(operation);
        }

        Ok(y_i)
    }
}

#[derive(Debug, Error)]
pub enum GroupError<ID, OP, C, RS, ORD, GS>
where
    ID: IdentityHandle,
    OP: OperationId + Ord,
    RS: Resolver<ORD::Message>,
    ORD: Ordering<ID, OP, GroupControlMessage<ID, OP, C>>,
    GS: GroupStore<ID, OP, C, RS, ORD>,
{
    #[error("duplicate operation {0} processed in group {1}")]
    DuplicateOperation(OP, ID),

    #[error("error occurred applying state change action")]
    StateChangeError(GroupMembershipError<GroupMember<ID>>),

    #[error("expected sub-group {0} to exist in the store")]
    MissingSubGroup(ID),

    #[error("resolver error: {0}")]
    ResolverError(RS::Error),

    #[error("ordering error: {0}")]
    OrderingError(ORD::Error),

    #[error("group store error: {0}")]
    GroupStoreError(GS::Error),

    #[error("states {0:?} not found in group {1}")]
    StatesNotFound(Vec<OP>, ID),

    #[error("expected dependencies {0:?} not found in group {1}")]
    DependenciesNotFound(Vec<OP>, ID),

    #[error("operation for group {0} processed in group {1}")]
    IncorrectGroupId(ID, ID),
}
