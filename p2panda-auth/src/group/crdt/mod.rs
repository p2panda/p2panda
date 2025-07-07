// SPDX-License-Identifier: MIT OR Apache-2.0

pub(crate) mod state;

use std::collections::{HashMap, HashSet};
use std::fmt::{Debug, Display};
use std::marker::PhantomData;

use petgraph::algo::toposort;
use petgraph::prelude::DiGraphMap;
use petgraph::visit::{DfsPostOrder, IntoNodeIdentifiers, NodeIndexable, Reversed};
use thiserror::Error;

use crate::access::Access;
use crate::group::{
    GroupAction, GroupControlMessage, GroupMember, GroupMembersState, GroupMembershipError,
};
use crate::traits::{
    GroupMembership, GroupStore, IdentityHandle, Operation, OperationId, Orderer, Resolver,
};

/// Error types for GroupCrdt.
#[derive(Debug, Error)]
pub enum GroupCrdtError<ID, OP, C, RS, ORD, GS>
where
    ID: IdentityHandle,
    OP: OperationId + Ord,
    RS: Resolver<ID, OP, C, ORD, GS>,
    ORD: Orderer<ID, OP, GroupControlMessage<ID, C>>,
    GS: GroupStore<ID, OP, C, RS, ORD>,
{
    #[error("duplicate operation {0} processed in group {1}")]
    DuplicateOperation(OP, ID),

    #[error("state change error processing operation {0}: {1:?}")]
    StateChangeError(OP, GroupMembershipError<GroupMember<ID>>),

    #[error("expected sub-group {0} to exist in the store")]
    MissingSubGroup(ID),

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

    #[error("operation id {0} exists in the graph but the corresponding operation was not found")]
    MissingOperation(OP),

    // TODO(glyph): I don't think this variant should live here. Maybe another error type?
    #[error("state not found for group member {0} in group {1}")]
    MemberNotFound(ID, ID),
}

/// State object for `GroupCrdt` containing the operation graph and all incremental group states.
///
/// Requires access to a global orderer and group store.
#[derive(Debug)]
#[cfg_attr(any(test, feature = "test_utils"), derive(Clone))]
pub struct GroupCrdtState<ID, OP, C, RS, ORD, GS>
where
    ID: IdentityHandle,
    OP: OperationId,
    ORD: Orderer<ID, OP, GroupControlMessage<ID, C>>,
    GS: GroupStore<ID, OP, C, RS, ORD>,
{
    /// ID of the local actor.
    pub my_id: ID,

    /// ID of the group.
    pub group_id: ID,

    /// Group state at every position in the operation graph.
    pub states: HashMap<OP, GroupMembersState<GroupMember<ID>, C>>,

    /// All operations processed by this group.
    pub operations: HashMap<OP, ORD::Operation>,

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

impl<ID, OP, C, RS, ORD, GS> GroupCrdtState<ID, OP, C, RS, ORD, GS>
where
    ID: IdentityHandle,
    OP: OperationId + Ord,
    C: Clone + Debug + PartialEq + PartialOrd,
    RS: Resolver<ID, OP, C, ORD, GS> + Debug,
    ORD: Orderer<ID, OP, GroupControlMessage<ID, C>> + Debug,
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

    /// Id of this group.
    pub fn id(&self) -> ID {
        self.group_id
    }

    /// Current tips for the group operation graph.
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

    /// Current tips of the group operation graph including all sub-groups.
    #[allow(clippy::type_complexity)]
    pub fn transitive_heads(&self) -> Result<HashSet<OP>, GroupCrdtError<ID, OP, C, RS, ORD, GS>> {
        let mut transitive_heads = self.heads();
        for (member, ..) in self.members() {
            if let GroupMember::Group(id) = member {
                let sub_group = self.get_sub_group(id)?;
                transitive_heads.extend(sub_group.transitive_heads()?);
            }
        }

        Ok(transitive_heads)
    }

    /// Current state of this group.
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

    fn state_at_inner(&self, dependencies: &HashSet<OP>) -> GroupMembersState<GroupMember<ID>, C> {
        let mut y = GroupMembersState::default();
        for id in dependencies.iter() {
            let Some(previous_y) = self.states.get(id) else {
                // We might be in a sub-group here processing dependencies which don't exist in
                // this graph, in that case we just ignore missing states.
                continue;
            };
            // Merge all dependency states from this group together.
            y = state::merge(previous_y.clone(), y);
        }

        y
    }

    /// Get the state of a group at a certain point in it's history.
    pub fn state_at(&self, dependencies: &HashSet<OP>) -> GroupMembersState<GroupMember<ID>, C> {
        self.state_at_inner(dependencies)
    }

    fn members_at_inner(&self, dependencies: &HashSet<OP>) -> Vec<(GroupMember<ID>, Access<C>)> {
        let y = self.state_at_inner(dependencies);
        y.members
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

    /// Get the group members at a certain point in groups history.
    pub fn members_at(&self, dependencies: &HashSet<OP>) -> Vec<(GroupMember<ID>, Access<C>)> {
        self.members_at_inner(dependencies)
    }

    #[allow(clippy::type_complexity)]
    fn transitive_members_at_inner(
        &self,
        dependencies: &HashSet<OP>,
    ) -> Result<Vec<(ID, Access<C>)>, GroupCrdtError<ID, OP, C, RS, ORD, GS>> {
        let mut members: HashMap<ID, Access<C>> = HashMap::new();

        // Get members of a group at a certain point in the groups history.
        for (member, root_access) in self.members_at_inner(dependencies) {
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
    #[allow(clippy::type_complexity)]
    pub fn transitive_members_at(
        &self,
        dependencies: &HashSet<OP>,
    ) -> Result<Vec<(ID, Access<C>)>, GroupCrdtError<ID, OP, C, RS, ORD, GS>> {
        let members = self.transitive_members_at_inner(dependencies)?;
        Ok(members)
    }

    /// Get all current members of the group.
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
    #[allow(clippy::type_complexity)]
    pub fn transitive_members(
        &self,
    ) -> Result<Vec<(ID, Access<C>)>, GroupCrdtError<ID, OP, C, RS, ORD, GS>> {
        let heads = self.transitive_heads()?;
        let members = self.transitive_members_at(&heads)?;

        Ok(members)
    }

    /// Get a sub group from the group store.
    #[allow(clippy::type_complexity)]
    pub(crate) fn get_sub_group(
        &self,
        id: ID,
    ) -> Result<GroupCrdtState<ID, OP, C, RS, ORD, GS>, GroupCrdtError<ID, OP, C, RS, ORD, GS>>
    {
        let y = self
            .group_store
            .get(&id)
            .map_err(|error| GroupCrdtError::GroupStoreError(error))?;

        // We expect that groups are created and correctly present in the store before we process
        // any messages requiring us to query them, so this error can only occur if there is an
        // error in any higher orchestration system.
        let Some(y) = y else {
            return Err(GroupCrdtError::MissingSubGroup(id));
        };

        Ok(y)
    }

    /// Get the maximum given access level for an actor at a certain point in the auth graph.
    ///
    /// An actor can be a direct individual member of a group, or a transitive member via a
    /// sub-group. This is a helper method which finds the hightest access level a member has and
    /// returns the group member which gives this actor the found access level.
    ///
    /// The passed dependencies array tells us which position in the graph to look at.
    #[allow(clippy::type_complexity)]
    pub fn max_access_identity(
        &self,
        actor: ID,
        dependencies: &HashSet<OP>,
    ) -> Result<Option<(GroupMember<ID>, Access<C>)>, GroupCrdtError<ID, OP, C, RS, ORD, GS>> {
        let mut access_levels = Vec::new();

        // Get members of a group at a certain point in the groups history.
        for (member, root_access) in self.members_at(dependencies) {
            match member {
                // If this is an individual matching the actor then push it to the access levels
                // vector.
                GroupMember::Individual(id) => {
                    if id == actor {
                        access_levels.push((member, root_access));
                    }
                }
                // If this is a group, then look into all transitive members to find any matches
                // to the passed actor id.
                GroupMember::Group(id) => {
                    let sub_group = self.get_sub_group(id)?;
                    if let Some((_, transitive_access)) = sub_group
                        // @TODO: we would prefer to call transitive_members() here so as to
                        //        account for the most recent sub-group state we know about. To do
                        //        this we first need to adjust how sub-group states are attached
                        //        to the root group graph.
                        .transitive_members_at(dependencies)
                        .unwrap()
                        .iter()
                        .find(|(member, _)| *member == actor)
                    {
                        // The actual access can't be greater than the access which was originally
                        // given to the sub-group.
                        let actual_access = if transitive_access < &root_access {
                            transitive_access.clone()
                        } else {
                            root_access.clone()
                        };
                        access_levels.push((member, actual_access));
                    }
                }
            }
        }

        let mut max_access: Option<(GroupMember<ID>, Access<C>)> = None;
        for (id, access) in access_levels {
            match max_access.clone() {
                Some((_, prev_access)) => {
                    if prev_access < access {
                        max_access = Some((id, access))
                    }
                }
                None => max_access = Some((id, access)),
            }
        }
        Ok(max_access)
    }
}

impl<ID, OP, C, RS, ORD, GS> GroupMembership<ID, OP, C> for GroupCrdtState<ID, OP, C, RS, ORD, GS>
where
    ID: IdentityHandle + Display,
    OP: OperationId + Ord + Display,
    C: Clone + Debug + PartialEq + PartialOrd,
    RS: Resolver<ID, OP, C, ORD, GS> + Debug,
    ORD: Orderer<ID, OP, GroupControlMessage<ID, C>> + Debug,
    GS: GroupStore<ID, OP, C, RS, ORD> + Debug,
{
    type State = GroupCrdtState<ID, OP, C, RS, ORD, GS>;

    type Error = GroupCrdtError<ID, OP, C, RS, ORD, GS>;

    /// Query the current access level of the given member.
    ///
    /// The member is expected to be a "stateless" individual, not a "stateful" group.
    fn access(
        y: &GroupCrdtState<ID, OP, C, RS, ORD, GS>,
        member: &ID,
    ) -> Result<Access<C>, GroupCrdtError<ID, OP, C, RS, ORD, GS>> {
        let member_state = y
            .transitive_members()?
            .into_iter()
            .find(|(member_id, _state)| member_id == member);

        if let Some(state) = member_state {
            let access = state.1.to_owned();

            Ok(access)
        } else {
            Err(GroupCrdtError::MemberNotFound(y.group_id, *member))
        }
    }

    /// Query group membership.
    fn member_ids(
        y: &GroupCrdtState<ID, OP, C, RS, ORD, GS>,
    ) -> Result<HashSet<ID>, GroupCrdtError<ID, OP, C, RS, ORD, GS>> {
        let member_ids = y
            .transitive_members()?
            .into_iter()
            .map(|(member_id, _state)| member_id)
            .collect();

        Ok(member_ids)
    }

    /// Return `true` if the given ID is an active member of the group.
    fn is_member(
        y: &GroupCrdtState<ID, OP, C, RS, ORD, GS>,
        member: &ID,
    ) -> Result<bool, GroupCrdtError<ID, OP, C, RS, ORD, GS>> {
        let member_state = y
            .transitive_members()?
            .into_iter()
            .find(|(member_id, _state)| member_id == member);

        let is_member = member_state.is_some();

        Ok(is_member)
    }

    /// Return `true` if the given member is currently assigned the `Pull` access level.
    fn is_puller(
        y: &GroupCrdtState<ID, OP, C, RS, ORD, GS>,
        member: &ID,
    ) -> Result<bool, GroupCrdtError<ID, OP, C, RS, ORD, GS>> {
        Ok(GroupCrdtState::access(y, member)?.is_pull())
    }

    /// Return `true` if the given member is currently assigned the `Read` access level.
    fn is_reader(
        y: &GroupCrdtState<ID, OP, C, RS, ORD, GS>,
        member: &ID,
    ) -> Result<bool, GroupCrdtError<ID, OP, C, RS, ORD, GS>> {
        Ok(GroupCrdtState::access(y, member)?.is_read())
    }

    /// Return `true` if the given member is currently assigned the `Write` access level.
    fn is_writer(
        y: &GroupCrdtState<ID, OP, C, RS, ORD, GS>,
        member: &ID,
    ) -> Result<bool, GroupCrdtError<ID, OP, C, RS, ORD, GS>> {
        Ok(GroupCrdtState::access(y, member)?.is_write())
    }

    /// Return `true` if the given member is currently assigned the `Manage` access level.
    fn is_manager(
        y: &GroupCrdtState<ID, OP, C, RS, ORD, GS>,
        member: &ID,
    ) -> Result<bool, GroupCrdtError<ID, OP, C, RS, ORD, GS>> {
        Ok(GroupCrdtState::access(y, member)?.is_manage())
    }
}

/// Core group CRDT for maintaining group membership state in a decentralized system.
///
/// Group members can be assigned different access levels, where only a sub-set of members can
/// mutate the state of the group itself. Group members can be (immutable) individuals or
/// (mutable) sub-groups.
///
/// The core data type is a Directed Acyclic Graph of operations containing group management
/// actions. Operations refer to the "previous" state (set of graph tips) which the action they
/// contain should be applied to; these references make up the edges in the graph. Additionally,
/// operations have a set of "dependencies" which could be part of any sub-group.
///
/// A requirement of the protocol is that all messages are processed in partial-order. When using
/// a dependency graph structure (as is the case in this implementation) it is possible to achieve
/// partial-ordering by only processing a message once all it's dependencies have themselves been
/// processed.
///
/// Group state is maintained using the state object `GroupMembersState`. Every time an action is
/// processed, a new state is generated and added to the map of all states. When a new operation
/// is received, it's "previous" state is calculated and then the message applied, resulting in a
/// new state.
///
/// Group membership rules are checked when an action is applied to the previous state, read more
/// in the `crdt::state` module.
///
/// The struct has several generic parameters which allow users to specify their own core types
/// and to customise behavior when handling concurrent changes when resolving a graph to it's
/// final state.
///
/// - ID : identifier for both an individual actor and group.
/// - OP : identifier for an operation.
/// - C  : conditions which restrict an access level.
/// - RS : generic resolver which contains logic for deciding when group state rebuilds are
///   required, and how concurrent actions are handled. See the `resolver` module for different
///   implementations.
/// - ORD: orderer which exposes an API for creating and processing operations with meta-data
///   which allow them to be processed in partial order.
/// - GS : global store containing states for all known groups.
#[derive(Clone, Debug, Default)]
pub struct GroupCrdt<ID, OP, C, RS, ORD, GS> {
    _phantom: PhantomData<(ID, OP, C, RS, ORD, GS)>,
}

impl<ID, OP, C, RS, ORD, GS> GroupCrdt<ID, OP, C, RS, ORD, GS>
where
    ID: IdentityHandle + Display,
    OP: OperationId + Ord + Display,
    C: Clone + Debug + PartialEq + PartialOrd,
    RS: Resolver<ID, OP, C, ORD, GS> + Debug,
    ORD: Orderer<ID, OP, GroupControlMessage<ID, C>> + Debug,
    ORD::Operation: Clone,
    GS: GroupStore<ID, OP, C, RS, ORD> + Clone + Debug,
{
    /// Prepare a next operation to be processed locally and sent to remote peers. An ORD
    /// implementation needs to ensure "previous" and "dependencies" are populated correctly so
    /// that a partial-order of all operations in the system can be established.
    ///
    /// The method `GroupCrdtState::heads` and `GroupCrdtState::transitive_heads` can be used to retrieve the
    /// operation ids of these operation dependencies.
    #[allow(clippy::type_complexity)]
    pub fn prepare(
        mut y: GroupCrdtState<ID, OP, C, RS, ORD, GS>,
        action: &GroupControlMessage<ID, C>,
    ) -> Result<
        (GroupCrdtState<ID, OP, C, RS, ORD, GS>, ORD::Operation),
        GroupCrdtError<ID, OP, C, RS, ORD, GS>,
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
    #[allow(clippy::type_complexity)]
    pub fn process(
        mut y: GroupCrdtState<ID, OP, C, RS, ORD, GS>,
        operation: &ORD::Operation,
    ) -> Result<GroupCrdtState<ID, OP, C, RS, ORD, GS>, GroupCrdtError<ID, OP, C, RS, ORD, GS>>
    {
        let operation_id = operation.id();
        let actor = operation.author();
        let control_message = operation.payload();
        let previous_operations = operation.previous();
        let dependencies = HashSet::from_iter(operation.dependencies().clone());
        let group_id = control_message.group_id();

        if y.group_id != group_id {
            // The operation is not intended for this group.
            return Err(GroupCrdtError::IncorrectGroupId(group_id, y.group_id));
        }

        if y.operations.contains_key(&operation_id) {
            // The operation has already been processed.
            return Err(GroupCrdtError::DuplicateOperation(operation_id, group_id));
        }

        // The resolver implementation contains the logic which determines when rebuilds are
        // required.
        let rebuild_required = RS::rebuild_required(&y, operation)?;

        // Add the new operation to the group state graph and operations vec. We validate it in
        // the following steps.
        y.graph.add_node(operation_id);
        for previous in &previous_operations {
            y.graph.add_edge(*previous, operation_id, ());
        }

        y.operations.insert(operation_id, operation.clone());

        let y_i = if rebuild_required {
            // Validate a concurrent operation against it's previous states.
            //
            // To do this we need to prune the graph to only include predecessor operations,
            // re-calculate the filter, and re-build all states.
            y = GroupCrdt::validate_concurrent_action(y, operation)?;

            // Process the group state with the provided resolver. This will populate the set of
            // messages which should be ignored when applying group management actions and also
            // rebuilds the group state (including the new operation).
            RS::process(y)?
        } else {
            // Compute the member's state by applying the new operation to the current group
            // state.
            //
            // This method validates that the actor has permission to perform the action.
            match Self::apply_action(
                y,
                operation_id,
                actor,
                &dependencies,
                &control_message.action,
            )? {
                StateChangeResult::Ok { state } => state,
                StateChangeResult::Noop { error, .. } => {
                    return Err(GroupCrdtError::StateChangeError(operation_id, error));
                }
                StateChangeResult::Filtered { .. } => {
                    // Operations can't be filtered out before they were processed.
                    unreachable!()
                }
            }
        };

        // Update the group in the store.
        y_i.group_store
            .insert(&group_id, &y_i)
            .map_err(|error| GroupCrdtError::GroupStoreError(error))?;

        Ok(y_i)
    }

    /// Apply an action to a single group state.
    #[allow(clippy::type_complexity)]
    pub(crate) fn apply_action(
        mut y: GroupCrdtState<ID, OP, C, RS, ORD, GS>,
        id: OP,
        actor: ID,
        dependencies: &HashSet<OP>,
        action: &GroupAction<ID, C>,
    ) -> Result<StateChangeResult<ID, OP, C, RS, ORD, GS>, GroupCrdtError<ID, OP, C, RS, ORD, GS>>
    {
        // Compute the member's state by applying the new operation to it's claimed "dependencies"
        // state.
        let members_y = if dependencies.is_empty() {
            GroupMembersState::default()
        } else {
            y.state_at(dependencies)
        };

        // Get the maximum access level for this actor.
        let max_access = y.max_access_identity(actor, dependencies)?;
        let member_id = match max_access {
            Some((id, _)) => id,
            None => GroupMember::Individual(actor),
        };

        // Only add the resulting member's state to the states map if the operation isn't
        // flagged to be ignored.
        if !y.ignore.contains(&id) {
            let result = match action.clone() {
                GroupAction::Add { member, access, .. } => {
                    state::add(members_y.clone(), member_id, member, access)
                }
                GroupAction::Remove { member, .. } => {
                    state::remove(members_y.clone(), member_id, member)
                }
                GroupAction::Promote { member, access } => {
                    state::promote(members_y.clone(), member_id, member, access)
                }
                GroupAction::Demote { member, access } => {
                    state::demote(members_y.clone(), member_id, member, access)
                }
                GroupAction::Create { initial_members } => Ok(state::create(&initial_members)),
            };

            match result {
                Ok(members_y_i) => y.states.insert(id, members_y_i),
                Err(err) => {
                    // Errors occur here because the member attempting to perform an action
                    // doesn't have a suitable access level, or that the action itself is invalid
                    // (eg. promoting a non-existent member).
                    //
                    // 1) We expect some errors to occur when when intentionally filtered out
                    //    actions cause later operations to become invalid.
                    //
                    // 2) Operations which other peers accepted into their graph _before_
                    //    receiving some concurrent operation which caused them to be invalid.
                    //
                    // In both cases it's critical that the action does not cause any state
                    // change, however we do want to accept them into our graph so as to ensure
                    // consistency consistency across peers.
                    y.states.insert(id, members_y);
                    return Ok(StateChangeResult::Noop {
                        state: y,
                        error: err,
                    });
                }
            };
        } else {
            y.states.insert(id, members_y);
            return Ok(StateChangeResult::Filtered { state: y });
        }
        Ok(StateChangeResult::Ok { state: y })
    }

    /// Validate an action by applying it to the group state build to it's previous pointers.
    ///
    /// When processing an new operation we need to validate that the contained action is valid
    /// before including it in the graph. By valid we mean that the author who composed the action
    /// had authority to perform the claimed action, and that the action fulfils all group change
    /// requirements. To check this we need to re-build the group state to the operations claimed
    /// "previous" state. This process involves pruning any operations which are not predecessors
    /// of the new operation resolving the group state again.
    ///
    /// This is a relatively expensive computation and should only be used when a re-build is
    /// actually required.
    #[allow(clippy::type_complexity)]
    pub(crate) fn validate_concurrent_action(
        mut y: GroupCrdtState<ID, OP, C, RS, ORD, GS>,
        operation: &ORD::Operation,
    ) -> Result<GroupCrdtState<ID, OP, C, RS, ORD, GS>, GroupCrdtError<ID, OP, C, RS, ORD, GS>>
    {
        // Keep hold of original operations and graph.
        let last_graph = y.graph.clone();
        let last_ignore = y.ignore.clone();
        let last_states = y.states.clone();

        // Collect predecessors of the new operation.
        let mut predecessors = HashSet::new();
        for previous in operation.previous() {
            let reversed = Reversed(&y.graph);
            let mut dfs_rev = DfsPostOrder::new(&reversed, previous);
            while let Some(id) = dfs_rev.next(&reversed) {
                predecessors.insert(id);
            }
        }

        // Remove all other nodes from the graph.
        let to_remove: Vec<_> = y
            .graph
            .node_identifiers()
            .filter(|n| !predecessors.contains(n))
            .collect();

        for node in &to_remove {
            y.graph.remove_node(*node);
        }

        y = RS::process(y)?;

        let dependencies = HashSet::from_iter(operation.dependencies().clone());

        let mut y_i = match Self::apply_action(
            y,
            operation.id(),
            operation.author(),
            &dependencies,
            &operation.payload().action,
        )? {
            StateChangeResult::Ok { state } => state,
            StateChangeResult::Noop { error, .. } => {
                return Err(GroupCrdtError::StateChangeError(operation.id(), error));
            }
            StateChangeResult::Filtered { .. } => {
                // Operations can't be filtered out before they were processed.
                unreachable!()
            }
        };

        y_i.graph = last_graph;
        y_i.ignore = last_ignore;
        y_i.states = last_states;

        Ok(y_i)
    }

    /// Rebuild the group state.
    ///
    /// This method assumes that a new filter has already calculated and added to the group state.
    /// No graph traversal occurs, all operations are simply iterated over and applied to the
    /// group state if they are not explicitly filtered. Errors resulting from "no-op" operations
    /// (operations which became invalid because they are transitively dependent on filtered
    /// operations) are expected and therefore not propagated further.
    #[allow(clippy::type_complexity)]
    pub(crate) fn rebuild(
        y: GroupCrdtState<ID, OP, C, RS, ORD, GS>,
    ) -> Result<GroupCrdtState<ID, OP, C, RS, ORD, GS>, GroupCrdtError<ID, OP, C, RS, ORD, GS>>
    {
        let mut y_i = GroupCrdtState::new(y.my_id, y.group_id, y.group_store, y.orderer_y);
        y_i.ignore = y.ignore;
        let operations = y.operations;

        let topo_sort =
            toposort(&y.graph, None).expect("group operation sets can be ordered topologically");

        // Apply every operation.
        let mut create_found = false;
        for operation_id in topo_sort {
            let operation = operations
                .get(&operation_id)
                .expect("all processed operations exist");
            let actor = operation.author();
            let operation_id = operation.id();
            let control_message = operation.payload();
            let group_id = control_message.group_id();
            let dependencies = HashSet::from_iter(operation.dependencies().clone());

            // Sanity check: we should only apply operations for this group.
            assert_eq!(y_i.group_id, group_id);

            // Sanity check: the first operation must be a create and all other operations must not be.
            if create_found {
                assert!(!control_message.is_create())
            } else {
                assert!(control_message.is_create())
            }

            create_found = true;

            y_i = match Self::apply_action(
                y_i,
                operation_id,
                actor,
                &dependencies,
                &control_message.action,
            )? {
                StateChangeResult::Ok { state } => state,
                StateChangeResult::Noop { state, .. } => {
                    // We don't error here as during re-build we expect some operations to
                    // fail if they've been transitively invalidated by a change in
                    // filter.
                    state
                }
                StateChangeResult::Filtered { state } => state,
            };

            // Add the new operation to the group state graph and operations vec.
            y_i.graph.add_node(operation_id);
            for previous in &operation.previous() {
                y_i.graph.add_edge(*previous, operation_id, ());
            }
        }

        y_i.operations = operations;
        Ok(y_i)
    }
}

/// Return types expected from applying an action to group state.
pub enum StateChangeResult<ID, OP, C, RS, ORD, GS>
where
    ID: IdentityHandle + Display,
    OP: OperationId + Ord + Display,
    C: Clone + Debug + PartialEq + PartialOrd,
    RS: Resolver<ID, OP, C, ORD, GS> + Debug,
    ORD: Orderer<ID, OP, GroupControlMessage<ID, C>> + Debug,
    ORD::Operation: Clone,
    GS: GroupStore<ID, OP, C, RS, ORD> + Clone + Debug,
{
    /// Action was applied and no error occurred.
    Ok {
        state: GroupCrdtState<ID, OP, C, RS, ORD, GS>,
    },

    /// Action was not applied because it failed internal validation.
    Noop {
        state: GroupCrdtState<ID, OP, C, RS, ORD, GS>,
        #[allow(unused)]
        error: GroupMembershipError<GroupMember<ID>>,
    },

    /// Action was not applied because it has been filtered out.
    Filtered {
        state: GroupCrdtState<ID, OP, C, RS, ORD, GS>,
    },
}

#[cfg(test)]
pub(crate) mod tests {

    use rand::SeedableRng;
    use rand::rngs::StdRng;

    use crate::Access;
    use crate::group::{
        GroupAction, GroupControlMessage, GroupCrdt, GroupCrdtError, GroupCrdtState, GroupMember,
        GroupMembershipError,
    };
    use crate::test_utils::{
        MessageId, Network, TestGroup, TestGroupState, TestGroupStore, TestOperation,
        TestOrdererState,
    };

    pub(crate) fn from_create(
        actor_id: char,
        group_id: char,
        op_create: &TestOperation,
        rng: &mut StdRng,
    ) -> TestGroupState {
        let store = TestGroupStore::default();
        let orderer = TestOrdererState::new(actor_id, store.clone(), StdRng::from_rng(rng));
        let group = TestGroupState::new(actor_id, group_id, store, orderer);
        TestGroup::process(group, op_create).unwrap()
    }

    pub(crate) fn create_group(
        actor_id: char,
        group_id: char,
        members: Vec<(char, Access<()>)>,
        rng: &mut StdRng,
    ) -> (TestGroupState, TestOperation) {
        let store = TestGroupStore::default();
        let orderer = TestOrdererState::new(actor_id, store.clone(), StdRng::from_rng(rng));
        let group = TestGroupState::new(actor_id, group_id, store, orderer);
        let control_message = GroupControlMessage {
            group_id,
            action: GroupAction::Create {
                initial_members: members
                    .into_iter()
                    .map(|(id, access)| (GroupMember::Individual(id), access))
                    .collect(),
            },
        };
        let (group, op) = TestGroup::prepare(group, &control_message).unwrap();
        let group = TestGroup::process(group, &op).unwrap();
        (group, op)
    }

    pub(crate) fn add_member(
        group: TestGroupState,
        group_id: char,
        member: char,
        access: Access<()>,
    ) -> (TestGroupState, TestOperation) {
        let control_message = GroupControlMessage {
            group_id,
            action: GroupAction::Add {
                member: GroupMember::Individual(member),
                access,
            },
        };
        let (group, op) = TestGroup::prepare(group, &control_message).unwrap();
        let group = TestGroup::process(group, &op).unwrap();
        (group, op)
    }

    pub(crate) fn remove_member(
        group: TestGroupState,
        group_id: char,
        member: char,
    ) -> (TestGroupState, TestOperation) {
        let control_message = GroupControlMessage {
            group_id,
            action: GroupAction::Remove {
                member: GroupMember::Individual(member),
            },
        };
        let (group, op) = TestGroup::prepare(group, &control_message).unwrap();
        let group = TestGroup::process(group, &op).unwrap();
        (group, op)
    }

    pub(crate) fn sync(group: TestGroupState, ops: &[TestOperation]) -> TestGroupState {
        ops.iter()
            .fold(group, |g, op| TestGroup::process(g, op).unwrap())
    }

    pub(crate) fn assert_members(
        group: &TestGroupState,
        expected: &[(GroupMember<char>, Access<()>)],
    ) {
        let mut actual = group.members();
        let mut expected = expected.to_vec();
        actual.sort();
        expected.sort();
        assert_eq!(actual, expected);
    }

    #[test]
    fn basic_group() {
        let group_id = '1';
        let alice = 'A';
        let store = TestGroupStore::default();
        let rng = StdRng::from_os_rng();
        let orderer_y = TestOrdererState::new(alice, store.clone(), rng);
        let group_y = TestGroupState::new(alice, group_id, store, orderer_y);

        // Create group with alice as initial admin member.
        let control_message_001 = GroupControlMessage {
            group_id,
            action: GroupAction::Create {
                initial_members: vec![(GroupMember::Individual(alice), Access::manage())],
            },
        };
        let (group_y, operation_001) = TestGroup::prepare(group_y, &control_message_001).unwrap();
        let group_y = TestGroup::process(group_y, &operation_001).unwrap();

        let mut members = group_y.members();
        members.sort();
        assert_eq!(
            members,
            vec![(GroupMember::Individual(alice), Access::manage())]
        );

        // Add bob with read access.
        let bob = 'B';
        let control_message_002 = GroupControlMessage {
            group_id,
            action: GroupAction::Add {
                member: GroupMember::Individual(bob),
                access: Access::read(),
            },
        };
        let (group_y, operation_002) = TestGroup::prepare(group_y, &control_message_002).unwrap();
        let group_y = TestGroup::process(group_y, &operation_002).unwrap();

        let mut members = group_y.members();
        members.sort();
        assert_eq!(
            members,
            vec![
                (GroupMember::Individual(alice), Access::manage()),
                (GroupMember::Individual(bob), Access::read())
            ]
        );

        // Add claire with write access.
        let claire = 'C';
        let control_message_003 = GroupControlMessage {
            group_id,
            action: GroupAction::Add {
                member: GroupMember::Individual(claire),
                access: Access::write(),
            },
        };
        let (group_y, operation_003) = TestGroup::prepare(group_y, &control_message_003).unwrap();
        let group_y = TestGroup::process(group_y, &operation_003).unwrap();

        let mut members = group_y.members();
        members.sort();
        assert_eq!(
            members,
            vec![
                (GroupMember::Individual(alice), Access::manage()),
                (GroupMember::Individual(bob), Access::read()),
                (GroupMember::Individual(claire), Access::write())
            ]
        );

        // Promote claire to admin.
        let control_message_004 = GroupControlMessage {
            group_id,
            action: GroupAction::Promote {
                member: GroupMember::Individual(claire),
                access: Access::manage(),
            },
        };
        let (group_y, operation_004) = TestGroup::prepare(group_y, &control_message_004).unwrap();
        let group_y = TestGroup::process(group_y, &operation_004).unwrap();

        let mut members = group_y.members();
        members.sort();
        assert_eq!(
            members,
            vec![
                (GroupMember::Individual(alice), Access::manage()),
                (GroupMember::Individual(bob), Access::read()),
                (GroupMember::Individual(claire), Access::manage())
            ]
        );

        // Demote bob to poll access.
        let control_message_005 = GroupControlMessage {
            group_id,
            action: GroupAction::Demote {
                member: GroupMember::Individual(bob),
                access: Access::pull(),
            },
        };
        let (group_y, operation_005) = TestGroup::prepare(group_y, &control_message_005).unwrap();
        let group_y = TestGroup::process(group_y, &operation_005).unwrap();

        let mut members = group_y.members();
        members.sort();
        assert_eq!(
            members,
            vec![
                (GroupMember::Individual(alice), Access::manage()),
                (GroupMember::Individual(bob), Access::pull()),
                (GroupMember::Individual(claire), Access::manage())
            ]
        );

        // Remove bob.
        let control_message_006 = GroupControlMessage {
            group_id,
            action: GroupAction::Remove {
                member: GroupMember::Individual(bob),
            },
        };
        let (group_y, operation_006) = TestGroup::prepare(group_y, &control_message_006).unwrap();
        let group_y = TestGroup::process(group_y, &operation_006).unwrap();

        let mut members = group_y.members();
        members.sort();
        assert_eq!(
            members,
            vec![
                (GroupMember::Individual(alice), Access::manage()),
                (GroupMember::Individual(claire), Access::manage())
            ]
        );
    }

    #[test]
    fn nested_groups() {
        let alice = 'A';
        let alice_mobile = 'M';
        let alice_laptop = 'L';

        let alice_devices_group = 'D';
        let alice_team_group = 'T';

        // The group store is shared state across all group instances.
        let store = TestGroupStore::default();
        let rng = StdRng::from_os_rng();
        let alice_orderer_y = TestOrdererState::new(alice, store.clone(), rng);

        // One devices group instance.
        let devices_group_y = GroupCrdtState::new(
            alice,
            alice_devices_group,
            store.clone(),
            alice_orderer_y.clone(),
        );

        // One team group instance.
        let team_group_y =
            GroupCrdtState::new(alice, alice_team_group, store.clone(), alice_orderer_y);

        // Control message creating the devices group, with alice, alice_laptop and alice mobile as members.
        let control_message_001 = GroupControlMessage {
            group_id: devices_group_y.id(),
            action: GroupAction::Create {
                initial_members: vec![
                    (GroupMember::Individual(alice), Access::manage()),
                    (GroupMember::Individual(alice_laptop), Access::manage()),
                    (GroupMember::Individual(alice_mobile), Access::write()),
                ],
            },
        };

        // Prepare the operation.
        let (devices_group_y, operation_001) =
            TestGroup::prepare(devices_group_y, &control_message_001).unwrap();

        // Process the operation.
        let devices_group_y = TestGroup::process(devices_group_y, &operation_001).unwrap();

        // alice, alice_laptop and alice_mobile are all members of the group.
        let mut members = devices_group_y.members();
        members.sort();
        assert_eq!(
            members,
            vec![
                (GroupMember::Individual(alice), Access::manage()),
                (GroupMember::Individual(alice_laptop), Access::manage()),
                (GroupMember::Individual(alice_mobile), Access::write()),
            ],
        );

        // Create alice's team group, with alice as the only member.
        let control_message_002 = GroupControlMessage {
            group_id: team_group_y.id(),
            action: GroupAction::Create {
                initial_members: vec![(GroupMember::Individual(alice), Access::manage())],
            },
        };

        // Prepare the operation.
        let (team_group_y, operation_002) =
            TestGroup::prepare(team_group_y, &control_message_002).unwrap();

        // Process it.
        let team_group_y = TestGroup::process(team_group_y, &operation_002).unwrap();

        // Add alice's devices group as a member of her teams group with read access.
        let control_message_003 = GroupControlMessage {
            group_id: team_group_y.id(),
            action: GroupAction::Add {
                member: GroupMember::Group(devices_group_y.id()),
                access: Access::read(),
            },
        };
        let (team_group_y, operation_003) =
            TestGroup::prepare(team_group_y, &control_message_003).unwrap();
        let team_group_y = TestGroup::process(team_group_y, &operation_003).unwrap();

        // Alice and the devices group are direct members of the team group.
        let mut members = team_group_y.members();
        members.sort();
        assert_eq!(
            members,
            vec![
                (GroupMember::Individual(alice), Access::manage()),
                (GroupMember::Group(alice_devices_group), Access::read())
            ]
        );

        // alice, alice_laptop and alice_mobile are transitive members, only alice has Manage access
        // (even though alice_laptop has Manage access to the devices sub-group).
        let mut transitive_members = team_group_y.transitive_members().unwrap();
        transitive_members.sort();
        assert_eq!(
            transitive_members,
            vec![
                (alice, Access::manage()),
                (alice_laptop, Access::read()),
                (alice_mobile, Access::read()),
            ]
        );
    }

    #[test]
    fn multi_user() {
        let alice = 'A';
        let bob = 'B';
        let claire = 'C';

        let alice_mobile = 'M';
        let alice_laptop = 'L';

        let alice_devices_group = 'D';
        let alice_team_group = 'T';

        let rng = StdRng::from_os_rng();
        // let rng = StdRng::from_seed([0u8; 32]);

        let mut network = Network::new([alice, bob, claire], rng);

        // Alice creates a team group with themselves as initial member.
        network.create(
            alice_team_group,
            alice,
            vec![(GroupMember::Individual(alice), Access::manage())],
        );

        // And then adds bob as manager.
        network.add(
            alice,
            GroupMember::Individual(bob),
            alice_team_group,
            Access::manage(),
        );

        // Everyone processes these operations.
        network.process();

        let alice_members = network.members(&alice, &alice_team_group);
        let bob_members = network.members(&bob, &alice_team_group);
        let claire_members = network.members(&claire, &alice_team_group);
        assert_eq!(
            alice_members,
            vec![
                (GroupMember::Individual('A'), Access::manage()),
                (GroupMember::Individual('B'), Access::manage()),
            ]
        );
        assert_eq!(alice_members, claire_members);
        assert_eq!(alice_members, bob_members);

        let alice_transitive_members = network.transitive_members(&alice, &alice_team_group);
        let bob_transitive_members = network.transitive_members(&bob, &alice_team_group);
        let claire_transitive_members = network.transitive_members(&claire, &alice_team_group);
        assert_eq!(
            alice_transitive_members,
            vec![('A', Access::manage()), ('B', Access::manage()),]
        );
        assert_eq!(alice_transitive_members, bob_transitive_members);
        assert_eq!(alice_transitive_members, claire_transitive_members);

        // Bob adds claire with read access.
        network.add(
            bob,
            GroupMember::Individual(claire),
            alice_team_group,
            Access::read(),
        );

        // Alice (concurrently) creates a devices group.
        network.create(
            alice_devices_group,
            alice,
            vec![
                (GroupMember::Individual(alice_mobile), Access::write()),
                (GroupMember::Individual(alice_laptop), Access::manage()),
            ],
        );

        // And adds it to the teams group.
        network.add(
            alice,
            GroupMember::Group(alice_devices_group),
            alice_team_group,
            Access::manage(),
        );

        // Everyone processes these operations.
        network.process();

        let alice_members = network.members(&alice, &alice_team_group);
        let bob_members = network.members(&bob, &alice_team_group);
        let claire_members = network.members(&claire, &alice_team_group);
        assert_eq!(
            alice_members,
            vec![
                (GroupMember::Individual('A'), Access::manage()),
                (GroupMember::Individual('B'), Access::manage()),
                (GroupMember::Individual('C'), Access::read()),
                (GroupMember::Group('D'), Access::manage())
            ]
        );
        assert_eq!(alice_members, bob_members);
        assert_eq!(alice_members, claire_members);

        let alice_transitive_members = network.transitive_members(&alice, &alice_team_group);
        let bob_transitive_members = network.transitive_members(&bob, &alice_team_group);
        let claire_transitive_members = network.transitive_members(&claire, &alice_team_group);
        assert_eq!(
            alice_transitive_members,
            vec![
                ('A', Access::manage()),
                ('B', Access::manage()),
                ('C', Access::read()),
                ('L', Access::manage()),
                ('M', Access::write())
            ]
        );
        assert_eq!(alice_transitive_members, bob_transitive_members);
        assert_eq!(alice_transitive_members, claire_transitive_members);
    }

    #[test]
    fn ooo() {
        let alice = 'A';
        let bob = 'B';
        let claire = 'C';

        let alice_friends = vec!['D', 'E', 'F'];
        let bob_friends = vec!['G', 'H', 'I'];
        let claire_friends = vec!['J', 'K', 'L'];

        let friends_group = 'T';

        let rng = StdRng::from_os_rng();
        // let rng = StdRng::from_seed([0u8; 32]);

        let mut network = Network::new([alice, bob, claire], rng);

        // Alice creates a friends group with themselves as initial member.
        network.create(
            friends_group,
            alice,
            vec![
                (GroupMember::Individual(alice), Access::manage()),
                (GroupMember::Individual(bob), Access::manage()),
                (GroupMember::Individual(claire), Access::manage()),
            ],
        );

        network.process();

        // alice, bob and claire all concurrently add 3 new friends, then remove one
        for friend in &alice_friends {
            network.add(
                alice,
                GroupMember::Individual(*friend),
                friends_group,
                Access::read(),
            );
        }

        network.remove(
            alice,
            GroupMember::Individual(alice_friends[0]),
            friends_group,
        );

        for friend in &bob_friends {
            network.add(
                bob,
                GroupMember::Individual(*friend),
                friends_group,
                Access::read(),
            );
        }

        network.remove(bob, GroupMember::Individual(bob_friends[0]), friends_group);

        for friend in &claire_friends {
            network.add(
                claire,
                GroupMember::Individual(*friend),
                friends_group,
                Access::read(),
            );
        }

        network.remove(
            claire,
            GroupMember::Individual(claire_friends[0]),
            friends_group,
        );

        // alice, bob and claire all process these messages in random orders.
        network.process_ooo();

        let alice_members = network.members(&alice, &friends_group);
        let bob_members = network.members(&bob, &friends_group);
        let claire_members = network.members(&claire, &friends_group);
        assert_eq!(
            alice_members,
            vec![
                (GroupMember::Individual('A'), Access::manage()),
                (GroupMember::Individual('B'), Access::manage()),
                (GroupMember::Individual('C'), Access::manage()),
                // (GroupMember::Individual('D'), Access::read()),
                (GroupMember::Individual('E'), Access::read()),
                (GroupMember::Individual('F'), Access::read()),
                // (GroupMember::Individual('G'), Access::read()),
                (GroupMember::Individual('H'), Access::read()),
                (GroupMember::Individual('I'), Access::read()),
                // (GroupMember::Individual('J'), Access::read()),
                (GroupMember::Individual('K'), Access::read()),
                (GroupMember::Individual('L'), Access::read()),
            ]
        );
        assert_eq!(alice_members, claire_members);
        assert_eq!(alice_members, bob_members);
    }

    #[test]
    fn add_remove_add() {
        let alice = 'A';
        let bob = 'B';

        let friends_group = 'T';

        let rng = StdRng::from_os_rng();
        // let rng = StdRng::from_seed([0u8; 32]);

        let mut network = Network::new([alice, bob], rng);

        network.create(
            friends_group,
            alice,
            vec![(GroupMember::Individual(alice), Access::manage())],
        );

        network.add(
            alice,
            GroupMember::Individual(bob),
            friends_group,
            Access::read(),
        );

        network.remove(alice, GroupMember::Individual(bob), friends_group);

        let members = network.members(&alice, &friends_group);
        assert_eq!(
            members,
            vec![(GroupMember::Individual('A'), Access::manage()),]
        );

        network.add(
            alice,
            GroupMember::Individual(bob),
            friends_group,
            Access::read(),
        );

        network.process();

        let members = network.members(&alice, &friends_group);
        assert_eq!(
            members,
            vec![
                (GroupMember::Individual('A'), Access::manage()),
                (GroupMember::Individual('B'), Access::read()),
            ]
        );
    }

    const ALICE: char = 'A';
    const BOB: char = 'B';
    const CHARLIE: char = 'C';
    const EDITH: char = 'E';
    const BOB_MOBILE: char = 'M';
    const BOB_LAPTOP: char = 'L';

    const BOB_DEVICES_GROUP: char = 'D';
    const CHARLIE_TEAM_GROUP: char = 'T';
    const ALICE_ORG_GROUP: char = 'O';

    // No concurrency in these test groups, the group store and orderer are shared across all group
    // instances.
    fn test_groups(rng: StdRng) -> (Network, Vec<MessageId>) {
        let mut network = Network::new([ALICE, BOB, CHARLIE], rng);
        let mut operations = vec![];

        let id = network.create(
            BOB_DEVICES_GROUP,
            BOB,
            vec![
                (GroupMember::Individual(BOB), Access::manage()),
                (GroupMember::Individual(BOB_LAPTOP), Access::write()),
            ],
        );
        operations.push(id);

        let id = network.add(
            BOB,
            GroupMember::Individual(BOB_MOBILE),
            BOB_DEVICES_GROUP,
            Access::read(),
        );
        operations.push(id);

        network.process();

        let id = network.create(
            CHARLIE_TEAM_GROUP,
            CHARLIE,
            vec![
                (GroupMember::Individual(CHARLIE), Access::manage()),
                (GroupMember::Individual(EDITH), Access::read()),
            ],
        );
        operations.push(id);

        let id = network.create(
            ALICE_ORG_GROUP,
            ALICE,
            vec![(GroupMember::Individual(ALICE), Access::manage())],
        );
        operations.push(id);

        network.process();

        let id = network.add(
            CHARLIE,
            GroupMember::Group(BOB_DEVICES_GROUP),
            CHARLIE_TEAM_GROUP,
            Access::manage(),
        );
        operations.push(id);

        network.process();

        let id = network.add(
            ALICE,
            GroupMember::Group(CHARLIE_TEAM_GROUP),
            ALICE_ORG_GROUP,
            Access::write(),
        );
        operations.push(id);

        network.process();

        (network, operations)
    }

    #[test]
    fn transitive_members() {
        let rng = StdRng::from_os_rng();
        let (network, _) = test_groups(rng);

        let expected_bob_devices_group_direct_members = vec![
            (GroupMember::Individual(BOB), Access::manage()),
            (GroupMember::Individual(BOB_LAPTOP), Access::write()),
            (GroupMember::Individual(BOB_MOBILE), Access::read()),
        ];

        let expected_bob_devices_group_transitive_members = vec![
            (BOB, Access::manage()),
            (BOB_LAPTOP, Access::write()),
            (BOB_MOBILE, Access::read()),
        ];

        let expected_charlie_team_group_direct_members = vec![
            (GroupMember::Individual(CHARLIE), Access::manage()),
            (GroupMember::Individual(EDITH), Access::read()),
            (GroupMember::Group(BOB_DEVICES_GROUP), Access::manage()),
        ];

        let expected_charlie_team_group_transitive_members = vec![
            (BOB, Access::manage()),
            (CHARLIE, Access::manage()),
            (EDITH, Access::read()),
            (BOB_LAPTOP, Access::write()),
            (BOB_MOBILE, Access::read()),
        ];

        let expected_alice_org_group_direct_members = vec![
            (GroupMember::Individual(ALICE), Access::manage()),
            (GroupMember::Group(CHARLIE_TEAM_GROUP), Access::write()),
        ];

        let expected_alice_org_group_transitive_members = vec![
            (ALICE, Access::manage()),
            (BOB, Access::write()),
            (CHARLIE, Access::write()),
            (EDITH, Access::read()),
            (BOB_LAPTOP, Access::write()),
            (BOB_MOBILE, Access::read()),
        ];

        let members = network.members(&BOB, &BOB_DEVICES_GROUP);
        assert_eq!(members, expected_bob_devices_group_direct_members);

        let transitive_members = network.transitive_members(&BOB, &BOB_DEVICES_GROUP);
        assert_eq!(
            transitive_members,
            expected_bob_devices_group_transitive_members
        );

        let members = network.members(&CHARLIE, &CHARLIE_TEAM_GROUP);
        assert_eq!(members, expected_charlie_team_group_direct_members);

        let transitive_members = network.transitive_members(&CHARLIE, &CHARLIE_TEAM_GROUP);
        assert_eq!(
            transitive_members,
            expected_charlie_team_group_transitive_members
        );

        let members = network.members(&ALICE, &ALICE_ORG_GROUP);
        assert_eq!(members, expected_alice_org_group_direct_members);

        let transitive_members = network.transitive_members(&ALICE, &ALICE_ORG_GROUP);
        assert_eq!(
            transitive_members,
            expected_alice_org_group_transitive_members
        );
    }

    #[test]
    fn members_at() {
        let rng = StdRng::from_os_rng();
        let (network, operations) = test_groups(rng);

        let create_devices_op_id = operations[0];
        let add_mobile_to_devices_op_id = operations[1];
        let create_team_op_id = operations[2];
        let create_org_op_id = operations[3];
        let add_devices_to_team_op_id = operations[4];
        let add_team_to_org_op_id = operations[5];

        // Initial state of the org group.
        let members =
            network.transitive_members_at(&ALICE, &ALICE_ORG_GROUP, &vec![create_org_op_id]);
        assert_eq!(members, vec![(ALICE, Access::manage())]);

        // CHARLIE_TEAM was added but before BOB_DEVICES was added to the team.
        let members = network.transitive_members_at(
            &ALICE,
            &ALICE_ORG_GROUP,
            &vec![add_team_to_org_op_id, create_team_op_id],
        );
        assert_eq!(
            members,
            vec![
                (ALICE, Access::manage()),
                (CHARLIE, Access::write()),
                (EDITH, Access::read())
            ]
        );

        // now BOB_DEVICES was added to the team.
        let members = network.transitive_members_at(
            &ALICE,
            &ALICE_ORG_GROUP,
            &vec![
                add_team_to_org_op_id,
                create_devices_op_id,
                add_devices_to_team_op_id,
            ],
        );
        assert_eq!(
            members,
            vec![
                (ALICE, Access::manage()),
                (BOB, Access::write()),
                (CHARLIE, Access::write()),
                (EDITH, Access::read()),
                (BOB_LAPTOP, Access::write()),
            ]
        );

        // now BOB_MOBILE was added to the devices group and we are at "current state".
        let members_at_most_recent_heads = network.transitive_members_at(
            &ALICE,
            &ALICE_ORG_GROUP,
            &vec![
                add_team_to_org_op_id,
                add_mobile_to_devices_op_id,
                add_devices_to_team_op_id,
            ],
        );
        assert_eq!(
            members_at_most_recent_heads,
            vec![
                (ALICE, Access::manage()),
                (BOB, Access::write()),
                (CHARLIE, Access::write()),
                (EDITH, Access::read()),
                (BOB_LAPTOP, Access::write()),
                (BOB_MOBILE, Access::read()),
            ]
        );

        // These queries should produce the same "current" member state.
        let current_members = network.transitive_members(&ALICE, &ALICE_ORG_GROUP);
        // This is a slightly strange thing to do, we are requesting the current state by passing in a
        // vec of all known operation ids. Logically it should produce the same state though.
        let members_by_all_known_operations =
            network.transitive_members_at(&ALICE, &ALICE_ORG_GROUP, &operations);

        assert_eq!(members_at_most_recent_heads, current_members);
        assert_eq!(
            members_at_most_recent_heads,
            members_by_all_known_operations
        );
    }
    #[test]
    fn error_cases() {
        let group_id = '0';
        let alice = 'A';
        let bob = 'B';
        let claire = 'C';
        let dave = 'D';
        let eve = 'E';

        let mut rng = StdRng::from_os_rng();

        let (y_i, _) = create_group(
            alice,
            group_id,
            vec![
                (alice, Access::manage()),
                (bob, Access::read()),
                (claire, Access::read()),
            ],
            &mut rng,
        );

        let previous: Vec<u32> = y_i.heads().into_iter().collect();

        // AlreadyAdded
        let op = TestOperation {
            id: 1,
            author: alice,
            dependencies: previous.clone(),
            previous: previous.clone(),
            payload: GroupControlMessage {
                group_id,
                action: GroupAction::Add {
                    member: GroupMember::Individual(bob),
                    access: Access::read(),
                },
            },
        };
        assert!(matches!(
            GroupCrdt::process(y_i.clone(), &op),
            Err(GroupCrdtError::StateChangeError(
                _,
                GroupMembershipError::AlreadyAdded(GroupMember::Individual('B'))
            ))
        ));

        // Remove claire so we can test AlreadyRemoved
        let y_ii = GroupCrdt::process(
            y_i,
            &TestOperation {
                id: 2,
                author: alice,
                dependencies: previous.clone(),
                previous: previous.clone(),
                payload: GroupControlMessage {
                    group_id,
                    action: GroupAction::Remove {
                        member: GroupMember::Individual(claire),
                    },
                },
            },
        )
        .unwrap();

        let previous: Vec<u32> = y_ii.heads().into_iter().collect();

        // AlreadyRemoved
        let op = TestOperation {
            id: 3,
            author: alice,
            dependencies: previous.clone(),
            previous: previous.clone(),
            payload: GroupControlMessage {
                group_id,
                action: GroupAction::Remove {
                    member: GroupMember::Individual(claire),
                },
            },
        };
        assert!(matches!(
            GroupCrdt::process(y_ii.clone(), &op),
            Err(GroupCrdtError::StateChangeError(
                _,
                GroupMembershipError::AlreadyRemoved(GroupMember::Individual('C'))
            ))
        ));

        // InsufficientAccess
        let op = TestOperation {
            id: 4,
            author: bob,
            dependencies: previous.clone(),
            previous: previous.clone(),
            payload: GroupControlMessage {
                group_id,
                action: GroupAction::Add {
                    member: GroupMember::Individual(dave),
                    access: Access::read(),
                },
            },
        };
        assert!(matches!(
            GroupCrdt::process(y_ii.clone(), &op),
            Err(GroupCrdtError::StateChangeError(
                _,
                GroupMembershipError::InsufficientAccess(GroupMember::Individual('B'))
            ))
        ));

        // Remove bob so we can test InactiveActor
        let y_iii = GroupCrdt::process(
            y_ii,
            &TestOperation {
                id: 5,
                author: alice,
                dependencies: previous.clone(),
                previous: previous.clone(),
                payload: GroupControlMessage {
                    group_id,
                    action: GroupAction::Remove {
                        member: GroupMember::Individual(bob),
                    },
                },
            },
        )
        .unwrap();

        let previous: Vec<u32> = y_iii.heads().into_iter().collect();

        // InactiveActor
        let op = TestOperation {
            id: 6,
            author: bob,
            dependencies: previous.clone(),
            previous: previous.clone(),
            payload: GroupControlMessage {
                group_id,
                action: GroupAction::Add {
                    member: GroupMember::Individual(dave),
                    access: Access::read(),
                },
            },
        };
        assert!(matches!(
            GroupCrdt::process(y_iii.clone(), &op),
            Err(GroupCrdtError::StateChangeError(
                _,
                GroupMembershipError::InactiveActor(GroupMember::Individual('B'))
            ))
        ));

        // InactiveMember
        let op = TestOperation {
            id: 7,
            author: alice,
            dependencies: previous.clone(),
            previous: previous.clone(),
            payload: GroupControlMessage {
                group_id,
                action: GroupAction::Promote {
                    member: GroupMember::Individual(claire),
                    access: Access::write(),
                },
            },
        };
        assert!(matches!(
            GroupCrdt::process(y_iii.clone(), &op),
            Err(GroupCrdtError::StateChangeError(
                _,
                GroupMembershipError::InactiveMember(GroupMember::Individual('C'))
            ))
        ));

        // UnrecognisedActor
        let op = TestOperation {
            id: 8,
            author: eve,
            dependencies: previous.clone(),
            previous: previous.clone(),
            payload: GroupControlMessage {
                group_id,
                action: GroupAction::Add {
                    member: GroupMember::Individual(dave),
                    access: Access::read(),
                },
            },
        };
        assert!(matches!(
            GroupCrdt::process(y_iii.clone(), &op),
            Err(GroupCrdtError::StateChangeError(
                _,
                GroupMembershipError::UnrecognisedActor(GroupMember::Individual('E'))
            ))
        ));

        // UnrecognisedMember
        let op = TestOperation {
            id: 9,
            author: alice,
            dependencies: previous.clone(),
            previous: previous.clone(),
            payload: GroupControlMessage {
                group_id,
                action: GroupAction::Promote {
                    member: GroupMember::Individual(eve),
                    access: Access::write(),
                },
            },
        };
        assert!(matches!(
            GroupCrdt::process(y_iii.clone(), &op),
            Err(GroupCrdtError::StateChangeError(
                _,
                GroupMembershipError::UnrecognisedMember(GroupMember::Individual('E'))
            ))
        ));
    }
    #[test]
    fn error_cases_resolver() {
        let group_id = '0';
        let alice = 'A';
        let bob = 'B';
        let claire = 'C';
        let dave = 'D';
        let eve = 'E';

        let mut rng = StdRng::from_os_rng();

        let (y_i, _) = create_group(
            alice,
            group_id,
            vec![
                (alice, Access::manage()),
                (bob, Access::read()),
                (claire, Access::read()),
            ],
            &mut rng,
        );

        let previous: Vec<u32> = y_i.heads().into_iter().collect();

        // Remove all current members and all all non-members as managers in a concurrent branch.
        let (mut y_ii, _) = remove_member(y_i, group_id, bob);
        (y_ii, _) = remove_member(y_ii, group_id, claire);
        (y_ii, _) = add_member(y_ii, group_id, dave, Access::manage());
        (y_ii, _) = add_member(y_ii, group_id, eve, Access::manage());
        (y_ii, _) = remove_member(y_ii, group_id, alice);

        let mut members = y_ii.members();
        members.sort();
        assert_eq!(
            members,
            vec![
                (GroupMember::Individual(dave), Access::manage()),
                (GroupMember::Individual(eve), Access::manage())
            ]
        );

        // All the following operations are appended into the group operation graph into a branch
        // concurrent to all the previous group changes. This means they should be validated against
        // state which does not include those changes (even though they are the "current" state).

        // AlreadyAdded (bob)
        let op = TestOperation {
            id: 1,
            author: alice,
            dependencies: previous.clone(),
            previous: previous.clone(),
            payload: GroupControlMessage {
                group_id,
                action: GroupAction::Add {
                    member: GroupMember::Individual(bob),
                    access: Access::read(),
                },
            },
        };
        assert!(matches!(
            GroupCrdt::process(y_ii.clone(), &op),
            Err(GroupCrdtError::StateChangeError(
                _,
                GroupMembershipError::AlreadyAdded(GroupMember::Individual('B'))
            ))
        ));

        // Remove claire
        let op = TestOperation {
            id: 2,
            author: alice,
            dependencies: previous.clone(),
            previous: previous.clone(),
            payload: GroupControlMessage {
                group_id,
                action: GroupAction::Remove {
                    member: GroupMember::Individual(claire),
                },
            },
        };
        let y_iii = GroupCrdt::process(y_ii.clone(), &op).unwrap();

        // Refer to only the newly published operation in previous so as to remain in the concurrent branch.
        let previous = vec![op.id];

        // AlreadyRemoved (claire)
        let op = TestOperation {
            id: 3,
            author: alice,
            dependencies: previous.clone(),
            previous: previous.clone(),
            payload: GroupControlMessage {
                group_id,
                action: GroupAction::Remove {
                    member: GroupMember::Individual(claire),
                },
            },
        };
        assert!(matches!(
            GroupCrdt::process(y_iii.clone(), &op),
            Err(GroupCrdtError::StateChangeError(
                _,
                GroupMembershipError::AlreadyRemoved(GroupMember::Individual('C'))
            ))
        ));

        // InsufficientAccess (bob tries to add dave)
        let op = TestOperation {
            id: 4,
            author: bob,
            dependencies: previous.clone(),
            previous: previous.clone(),
            payload: GroupControlMessage {
                group_id,
                action: GroupAction::Add {
                    member: GroupMember::Individual(dave),
                    access: Access::read(),
                },
            },
        };
        assert!(matches!(
            GroupCrdt::process(y_iii.clone(), &op),
            Err(GroupCrdtError::StateChangeError(
                _,
                GroupMembershipError::InsufficientAccess(GroupMember::Individual('B'))
            ))
        ));

        // Remove bob
        let op = TestOperation {
            id: 5,
            author: alice,
            dependencies: previous.clone(),
            previous: previous.clone(),
            payload: GroupControlMessage {
                group_id,
                action: GroupAction::Remove {
                    member: GroupMember::Individual(bob),
                },
            },
        };
        let y_iv = GroupCrdt::process(y_iii.clone(), &op).unwrap();

        // Refer to only the newly published operation in previous so as to remain in the concurrent branch.
        let previous = vec![op.id];

        // InactiveActor (bob tries to add dave)
        let op = TestOperation {
            id: 6,
            author: bob,
            dependencies: previous.clone(),
            previous: previous.clone(),
            payload: GroupControlMessage {
                group_id,
                action: GroupAction::Add {
                    member: GroupMember::Individual(dave),
                    access: Access::read(),
                },
            },
        };
        assert!(matches!(
            GroupCrdt::process(y_iv.clone(), &op),
            Err(GroupCrdtError::StateChangeError(
                _,
                GroupMembershipError::InactiveActor(GroupMember::Individual('B'))
            ))
        ));

        // InactiveMember (claire promoted)
        let op = TestOperation {
            id: 7,
            author: alice,
            dependencies: previous.clone(),
            previous: previous.clone(),
            payload: GroupControlMessage {
                group_id,
                action: GroupAction::Promote {
                    member: GroupMember::Individual(claire),
                    access: Access::write(),
                },
            },
        };
        assert!(matches!(
            GroupCrdt::process(y_iv.clone(), &op),
            Err(GroupCrdtError::StateChangeError(
                _,
                GroupMembershipError::InactiveMember(GroupMember::Individual('C'))
            ))
        ));

        // UnrecognisedActor (eve tries to add dave)
        let op = TestOperation {
            id: 8,
            author: eve,
            dependencies: previous.clone(),
            previous: previous.clone(),
            payload: GroupControlMessage {
                group_id,
                action: GroupAction::Add {
                    member: GroupMember::Individual(dave),
                    access: Access::read(),
                },
            },
        };
        assert!(matches!(
            GroupCrdt::process(y_iv.clone(), &op),
            Err(GroupCrdtError::StateChangeError(
                _,
                GroupMembershipError::UnrecognisedActor(GroupMember::Individual('E'))
            ))
        ));

        // UnrecognisedMember (alice promotes eve)
        let op = TestOperation {
            id: 9,
            author: alice,
            dependencies: previous.clone(),
            previous: previous.clone(),
            payload: GroupControlMessage {
                group_id,
                action: GroupAction::Promote {
                    member: GroupMember::Individual(eve),
                    access: Access::write(),
                },
            },
        };
        assert!(matches!(
            GroupCrdt::process(y_iv.clone(), &op),
            Err(GroupCrdtError::StateChangeError(
                _,
                GroupMembershipError::UnrecognisedMember(GroupMember::Individual('E'))
            ))
        ));
    }
}
