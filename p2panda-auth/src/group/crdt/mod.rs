// SPDX-License-Identifier: MIT OR Apache-2.0

pub(crate) mod state;

use std::collections::{HashMap, HashSet};
use std::fmt::Debug;
use std::marker::PhantomData;

use petgraph::prelude::DiGraphMap;
use petgraph::visit::{DfsPostOrder, IntoNodeIdentifiers, NodeIndexable, Reversed};
use thiserror::Error;

use crate::access::Access;
use crate::group::{
    GroupAction, GroupControlMessage, GroupMember, GroupMembersState, GroupMembershipError,
};
use crate::traits::{Conditions, IdentityHandle, Operation, OperationId, Orderer, Resolver};

/// Max depth of group nesting allowed.
///
/// Depth is checked during group state queries and if the depth is exceeded further additions are
/// ignored. The main reason for this check is to protect against accidental group nesting cycles
/// which may occur as a result of concurrent operations.
const MAX_NESTED_DEPTH: u32 = 1000;

/// Inner error types for GroupCrdt.
#[derive(Debug, Error)]
pub enum GroupCrdtInnerError<OP> {
    #[error("states {0:?} not found")]
    StatesNotFound(Vec<OP>),
}

/// Error types for GroupCrdt.
#[derive(Debug, Error)]
pub enum GroupCrdtError<ID, OP, C, RS, ORD>
where
    ID: IdentityHandle,
    OP: OperationId + Ord,
    RS: Resolver<ID, OP, C, ORD::Operation>,
    ORD: Orderer<ID, OP, GroupControlMessage<ID, C>> + Debug,
{
    #[error(transparent)]
    Inner(#[from] GroupCrdtInnerError<OP>),

    #[error("duplicate operation {0} processed in group {1}")]
    DuplicateOperation(OP, ID),

    #[error("group cycle detected adding {0} to {1} operation={2}")]
    GroupCycle(ID, ID, OP),

    #[error("state change error processing operation {0}: {1:?}")]
    StateChangeError(OP, GroupMembershipError<GroupMember<ID>>),

    #[error("attempted to add group {0} with manage access")]
    ManagerGroupsNotAllowed(ID),

    #[error("orderer error: {0}")]
    Orderer(ORD::Error),

    #[error("resolver error: {0}")]
    Resolver(RS::Error),
}

pub(crate) type GroupStates<ID, C> = HashMap<ID, GroupMembersState<GroupMember<ID>, C>>;

/// Inner state object for `GroupCrdt` which contains the actual groups state,
/// including operation graph and membership snapshots.
#[derive(Debug)]
#[cfg_attr(any(test, feature = "test_utils"), derive(Clone))]
pub struct GroupCrdtInnerState<ID, OP, C, M>
where
    ID: IdentityHandle,
    OP: OperationId + Ord,
{
    /// All operations processed by this group.
    pub operations: HashMap<OP, M>,

    /// All operations who's actions should be ignored.
    pub ignore: HashSet<OP>,

    /// All operations which are part of a mutual remove cycle.
    pub mutual_removes: HashSet<OP>,

    /// All resolved states.
    pub states: HashMap<OP, GroupStates<ID, C>>,

    /// Operation graph of all auth operations.
    pub graph: DiGraphMap<OP, ()>,
}

impl<ID, OP, C, M> Default for GroupCrdtInnerState<ID, OP, C, M>
where
    ID: IdentityHandle,
    OP: OperationId + Ord,
{
    fn default() -> Self {
        Self {
            operations: Default::default(),
            ignore: Default::default(),
            mutual_removes: Default::default(),
            states: Default::default(),
            graph: Default::default(),
        }
    }
}

impl<ID, OP, C, M> GroupCrdtInnerState<ID, OP, C, M>
where
    ID: IdentityHandle,
    OP: OperationId + Ord,
    C: Conditions,
    M: Operation<ID, OP, GroupControlMessage<ID, C>>,
{
    /// Current tips for the groups operation graph.
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

    /// Current group states.
    ///
    /// This method gets the state at all graph tips and then merges them together into one new
    /// state which represents the current state of the groups.
    pub fn current_state(&self) -> GroupStates<ID, C> {
        self.merge_states(&self.heads())
            .expect("states exist for processed operations")
    }

    /// Get the state at a certain point in history.
    pub fn state_at(
        &self,
        dependencies: &HashSet<OP>,
    ) -> Result<GroupStates<ID, C>, GroupCrdtInnerError<OP>> {
        self.merge_states(dependencies)
    }

    /// Merge multiple states together.
    fn merge_states(
        &self,
        ids: &HashSet<OP>,
    ) -> Result<GroupStates<ID, C>, GroupCrdtInnerError<OP>> {
        let mut current_state = HashMap::new();
        for id in ids {
            // Unwrap as this method is only used internally where all requested states should exist.
            let group_states = match self.states.get(id) {
                Some(group_states) => group_states.clone(),
                None => {
                    return Err(GroupCrdtInnerError::StatesNotFound(
                        ids.iter().cloned().collect(),
                    ));
                }
            };
            for (id, state) in group_states.into_iter() {
                current_state
                    .entry(id)
                    .and_modify(
                        |current_state: &mut GroupMembersState<GroupMember<ID>, C>| {
                            *current_state = state::merge(state.clone(), current_state.clone())
                        },
                    )
                    .or_insert(state);
            }
        }
        Ok(current_state)
    }

    fn members_inner(
        &self,
        group_id: ID,
        members: &mut HashMap<ID, Access<C>>,
        root_access: Option<Access<C>>,
        mut depth: u32,
    ) {
        // If we reached max nesting depth exit from the traversal.
        if depth == MAX_NESTED_DEPTH {
            return;
        }
        depth += 1;

        let current_states = self.current_state();
        let Some(group_state) = current_states.get(&group_id) else {
            return;
        };

        for (member, access) in group_state.access_levels() {
            // As we recurse into sub-groups we must assure that the newly
            // assignable access level is never higher than the previous root
            // access level. To do this we take whichever is less.
            let next_access = match root_access.clone() {
                Some(root_access) => {
                    if access <= root_access {
                        access.clone()
                    } else {
                        root_access
                    }
                }
                None => access.clone(),
            };

            match member {
                GroupMember::Individual(id) => {
                    // If this is an individual member, then add them straight to the members map.
                    members
                        .entry(id)
                        .and_modify(|current_access| {
                            // If the transitive access level this member holds (the access
                            // level the member has in it's sub-group) is greater than it's
                            // current access level, but not greater than the root access
                            // level (the access level initially assigned from the parent
                            // group) then update the access level.

                            // @TODO: we need to combine access levels here,
                            // which requires adding a trait bound to conditions
                            // which allows combining them as well. Or we return
                            // an array of access levels for each peer.
                            if *current_access < next_access {
                                *current_access = next_access.clone();
                            }
                        })
                        .or_insert_with(|| next_access);
                }
                GroupMember::Group(id) => self.members_inner(id, members, Some(next_access), depth),
            }
        }
    }

    /// Get all current members of a group.
    pub fn members(&self, group_id: ID) -> Vec<(ID, Access<C>)> {
        let mut members = HashMap::new();
        self.members_inner(group_id, &mut members, None, 0);
        members.into_iter().collect()
    }

    pub(crate) fn would_create_cycle(&self, operation: &M) -> bool {
        let control_message = operation.payload();
        let parent_group_id = control_message.group_id();

        if let GroupAction::Add {
            member: GroupMember::Group(child_group_id),
            ..
        } = &operation.payload().action
        {
            let states = self.current_state();
            let mut stack = vec![*child_group_id];
            let mut visited = HashSet::new();

            while let Some(child_group_id) = stack.pop() {
                if !visited.insert(child_group_id) {
                    continue;
                }
                if child_group_id == parent_group_id {
                    // Found a path from child group to parent.
                    return true;
                }
                if let Some(group_state) = states.get(&child_group_id) {
                    for (member, _) in group_state.access_levels() {
                        if let GroupMember::Group(id) = member {
                            stack.push(id);
                        }
                    }
                }
            }
        }

        false
    }
}

/// State object for `GroupCrdt` containing an orderer state and the inner
/// state.
#[derive(Debug)]
#[cfg_attr(any(test, feature = "test_utils"), derive(Clone))]
pub struct GroupCrdtState<ID, OP, C, ORD>
where
    ID: IdentityHandle,
    OP: OperationId + Ord,
    ORD: Orderer<ID, OP, GroupControlMessage<ID, C>> + Debug,
    ORD::Operation: Clone,
{
    /// Inner groups state.
    pub inner: GroupCrdtInnerState<ID, OP, C, ORD::Operation>,

    /// State for the orderer.
    pub orderer_y: ORD::State,
}

impl<ID, OP, C, ORD> GroupCrdtState<ID, OP, C, ORD>
where
    ID: IdentityHandle,
    OP: OperationId + Ord,
    C: Conditions,
    ORD: Orderer<ID, OP, GroupControlMessage<ID, C>> + Debug,
    ORD::Operation: Clone,
{
    /// Instantiate a new state.
    pub fn new(orderer_y: ORD::State) -> Self {
        Self {
            inner: GroupCrdtInnerState::default(),
            orderer_y,
        }
    }

    /// Get all direct members of a group.
    ///
    /// This method does not recurse into sub-groups, but rather returns only
    /// the direct group members and their access levels.
    pub fn root_members(&self, group_id: ID) -> Vec<(GroupMember<ID>, Access<C>)> {
        match self.inner.current_state().get(&group_id) {
            Some(group_y) => group_y.access_levels(),
            None => vec![],
        }
    }

    /// Get all transitive members of a group.
    ///
    /// This method recurses into all sub-groups and returns a resolved list of
    /// individual group members and their access levels.
    pub fn members(&self, group_id: ID) -> Vec<(ID, Access<C>)> {
        self.inner.members(group_id)
    }

    /// Returns `true` if the passed group exists in the current state.
    pub fn has_group(&self, group_id: ID) -> bool {
        self.inner.current_state().contains_key(&group_id)
    }
}

/// Core group CRDT for maintaining group membership state in a decentralized
/// system.
///
/// Group members can be assigned different access levels, where only a sub-set
/// of members can mutate the state of the group itself. Group members can be
/// (immutable) individuals or (mutable) sub-groups.
///
/// The core data type is a Directed Acyclic Graph of all operations containing
/// group management actions. Operations refer to the previous global state (set
/// of graph tips) in their "dependencies" field, this is the local state when
/// an actor creates a new auth action; these references make up the edges in
/// the graph.
///
/// A requirement of the protocol is that all messages are processed in
/// partial-order. When using a dependency graph structure (as is the case in
/// this implementation) it is possible to achieve partial-ordering by only
/// processing a message once all it's dependencies have themselves been
/// processed.
///
/// Group state is maintained using the state object `GroupMembersState`. Every
/// time an action is processed, a new state is generated and added to the map
/// of all states. When a new operation is received, it's previous state is
/// calculated and then the message applied, resulting in a new state.
///
/// Group membership rules are checked when an action is applied to the previous
/// state, read more in the `crdt::state` module.
///
/// The struct has several generic parameters which allow users to specify their
/// own core types and to customise behavior when handling concurrent changes
/// when resolving a graph to it's final state.
///
/// - ID : identifier for both an individual actor and group.
/// - OP : identifier for an operation.
/// - C  : conditions which restrict an access level.
/// - RS : generic resolver which contains logic for deciding when group state
///   rebuilds are required, and how concurrent actions are handled. See the
///   `resolver` module for different implementations.
/// - ORD: orderer which exposes an API for creating and processing operations
///   with meta-data which allow them to be processed in partial order.
#[derive(Clone, Debug, Default)]
pub struct GroupCrdt<ID, OP, C, RS, ORD> {
    _phantom: PhantomData<(ID, OP, C, RS, ORD)>,
}

impl<ID, OP, C, RS, ORD> GroupCrdt<ID, OP, C, RS, ORD>
where
    ID: IdentityHandle,
    OP: OperationId + Ord,
    C: Conditions,
    RS: Resolver<ID, OP, C, ORD::Operation, State = GroupCrdtInnerState<ID, OP, C, ORD::Operation>>,
    ORD: Orderer<ID, OP, GroupControlMessage<ID, C>> + Debug,
    ORD::Operation: Clone,
{
    pub fn init(orderer_y: ORD::State) -> GroupCrdtState<ID, OP, C, ORD> {
        GroupCrdtState {
            inner: GroupCrdtInnerState::default(),
            orderer_y,
        }
    }

    /// Prepare a next operation to be processed locally and sent to remote
    /// peers. An ORD implementation needs to ensure "dependencies" are
    /// populated correctly so that a partial-order of all operations in the
    /// system can be established.
    #[allow(clippy::type_complexity)]
    pub fn prepare(
        mut y: GroupCrdtState<ID, OP, C, ORD>,
        action: &GroupControlMessage<ID, C>,
    ) -> Result<(GroupCrdtState<ID, OP, C, ORD>, ORD::Operation), GroupCrdtError<ID, OP, C, RS, ORD>>
    {
        // Get the next operation from our global orderer.
        let ordering_y = y.orderer_y;
        let (ordering_y, operation) =
            ORD::next_message(ordering_y, action).map_err(GroupCrdtError::Orderer)?;
        y.orderer_y = ordering_y;
        Ok((y, operation))
    }

    /// Process an operation created locally or received from a remote peer.
    #[allow(clippy::type_complexity)]
    pub fn process(
        mut y: GroupCrdtState<ID, OP, C, ORD>,
        operation: &ORD::Operation,
    ) -> Result<GroupCrdtState<ID, OP, C, ORD>, GroupCrdtError<ID, OP, C, RS, ORD>> {
        let operation_id = operation.id();
        let actor = operation.author();
        let control_message = operation.payload();
        let dependencies = HashSet::from_iter(operation.dependencies().clone());
        let group_id = control_message.group_id();
        let rebuild_required =
            RS::rebuild_required(&y.inner, operation).map_err(GroupCrdtError::Resolver)?;

        // Validate that the author of this operation had the required access rights at the point
        // in the auth graph which they claim as their last state (the state at "dependencies").
        // It could be that they had access at this point but concurrent changes (which we know
        // about) mean that they have lost that access level. This case is dealt with later, here
        // we want to catch malicious or invalid operations which should _never_ be attached to
        // the graph.
        y = GroupCrdt::validate(y, operation)?;
        y = Self::add_operation(y, operation);

        if rebuild_required {
            y.inner = RS::process(y.inner).map_err(GroupCrdtError::Resolver)?;
            return Ok(y);
        }

        // We don't need to check the state change result as validation was already performed
        // above.
        let mut groups_y = y.inner.state_at(&dependencies)?;
        groups_y = apply_action(
            groups_y,
            group_id,
            operation_id,
            actor,
            &control_message.action,
            &y.inner.ignore,
        )
        .state()
        .to_owned();

        y.inner.states.insert(operation_id, groups_y);

        Ok(y)
    }

    /// Validate an action by applying it to the group state build to it's previous pointers.
    ///
    /// When processing a new operation we need to validate that the contained action is valid
    /// before including it in the graph. By valid we mean that the author who composed the action
    /// had authority to perform the claimed action, and that the action fulfils all group change
    /// requirements. To check this we need to re-build the group state to the operations claimed
    /// previous state. This process involves pruning any operations which are not predecessors of
    /// the new operation resolving the group state again.
    ///
    /// This is a relatively expensive computation and should only be used when a re-build is
    /// actually required.
    #[allow(clippy::type_complexity)]
    pub(crate) fn validate(
        y: GroupCrdtState<ID, OP, C, ORD>,
        operation: &ORD::Operation,
    ) -> Result<GroupCrdtState<ID, OP, C, ORD>, GroupCrdtError<ID, OP, C, RS, ORD>> {
        // Detect already processed operations.
        if y.inner.operations.contains_key(&operation.id()) {
            // The operation has already been processed.
            return Err(GroupCrdtError::DuplicateOperation(
                operation.id(),
                operation.payload().group_id(),
            ));
        }

        // Adding a group as a manager of another group is currently not
        // supported.
        //
        // @TODO: To support this behavior updates in the StrongRemove resolver
        // so that cross-group concurrent remove cycles are detected. Related to
        // issue: https://github.com/p2panda/p2panda/issues/779
        match &operation.payload().action {
            GroupAction::Add { member, access } | GroupAction::Promote { member, access } => {
                if member.is_group() && access.is_manage() {
                    return Err(GroupCrdtError::ManagerGroupsNotAllowed(member.id()));
                }
            }
            _ => (),
        };

        let last_graph = y.inner.graph.clone();
        let last_ignore = y.inner.ignore.clone();
        let last_mutual_removes = y.inner.mutual_removes.clone();
        let last_states = y.inner.states.clone();

        let dependencies = HashSet::from_iter(operation.dependencies().clone());

        // If this operation is concurrent to our current local state we need to rebuild the graph
        // to the operations' claimed dependencies in order to validate it correctly.
        let temp_y = if y.inner.heads() != dependencies {
            let mut temp_y = y;

            // Collect predecessors of the new operation.
            let mut predecessors = HashSet::new();
            for dependency in operation.dependencies() {
                let reversed = Reversed(&temp_y.inner.graph);
                let mut dfs_rev = DfsPostOrder::new(&reversed, dependency);
                while let Some(id) = dfs_rev.next(&reversed) {
                    predecessors.insert(id);
                }
            }

            // Remove all other nodes from the graph.
            let to_remove: Vec<_> = temp_y
                .inner
                .graph
                .node_identifiers()
                .filter(|n| !predecessors.contains(n))
                .collect();

            for node in &to_remove {
                temp_y.inner.graph.remove_node(*node);
            }

            temp_y.inner = RS::process(temp_y.inner).map_err(GroupCrdtError::Resolver)?;
            temp_y
        } else {
            y
        };

        // Detect if this operation would cause a nested group cycle.
        if temp_y.inner.would_create_cycle(operation) {
            let parent_group = operation.payload().group_id();

            // Only adds cause a cycle, we just access the member id here.
            let GroupAction::Add {
                member: sub_group, ..
            } = operation.payload().action
            else {
                unreachable!()
            };

            return Err(GroupCrdtError::GroupCycle(
                parent_group,
                sub_group.id(),
                operation.id(),
            ));
        }

        // Apply the operation onto the temporary state.
        let result = apply_action(
            temp_y.inner.current_state(),
            operation.payload().group_id(),
            operation.id(),
            operation.author(),
            &operation.payload().action,
            &temp_y.inner.ignore,
        );

        match result {
            StateChangeResult::Ok { state } => state,
            StateChangeResult::Error { error, .. } => {
                // Noop shouldn't happen when processing new operations as the
                // rebuild logic should have occurred instead.
                return Err(GroupCrdtError::StateChangeError(operation.id(), error));
            }
            StateChangeResult::Filtered { .. } => {
                // Operations can't be filtered out before they were processed.
                unreachable!();
            }
        };

        let mut y = temp_y;
        y.inner.graph = last_graph;
        y.inner.ignore = last_ignore;
        y.inner.mutual_removes = last_mutual_removes;
        y.inner.states = last_states;

        Ok(y)
    }

    /// Add an operation to the auth graph and operation map.
    ///
    /// NOTE: this method _does not_ process the operation so no new state is derived.
    fn add_operation(
        mut y: GroupCrdtState<ID, OP, C, ORD>,
        operation: &ORD::Operation,
    ) -> GroupCrdtState<ID, OP, C, ORD> {
        let operation_id = operation.id();
        let dependencies = operation.dependencies();

        // Add operation to the global auth graph.
        y.inner.graph.add_node(operation_id);
        for dependency in &dependencies {
            y.inner.graph.add_edge(*dependency, operation_id, ());
        }

        // Insert operation into all operations map.
        y.inner.operations.insert(operation_id, operation.clone());

        y
    }
}

/// Apply an action to a single group state.
pub(crate) fn apply_action<ID, OP, C>(
    mut groups_y: GroupStates<ID, C>,
    group_id: ID,
    id: OP,
    actor: ID,
    action: &GroupAction<ID, C>,
    filter: &HashSet<OP>,
) -> StateChangeResult<ID, C>
where
    ID: IdentityHandle,
    OP: OperationId + Ord,
    C: Conditions,
{
    let members_y = if action.is_create() {
        GroupMembersState::default()
    } else {
        groups_y
            .remove(&group_id)
            .expect("group already present in states map")
    };

    if filter.contains(&id) {
        groups_y.insert(group_id, members_y);
        return StateChangeResult::Filtered { state: groups_y };
    }

    let result = match action.clone() {
        GroupAction::Add { member, access, .. } => state::add(
            members_y.clone(),
            GroupMember::Individual(actor),
            member,
            access,
        ),
        GroupAction::Remove { member, .. } => {
            state::remove(members_y.clone(), GroupMember::Individual(actor), member)
        }
        GroupAction::Promote { member, access } => state::promote(
            members_y.clone(),
            GroupMember::Individual(actor),
            member,
            access,
        ),
        GroupAction::Demote { member, access } => state::demote(
            members_y.clone(),
            GroupMember::Individual(actor),
            member,
            access,
        ),
        GroupAction::Create { initial_members } => Ok(state::create(&initial_members)),
    };

    match result {
        Ok(members_y_i) => {
            groups_y.insert(group_id, members_y_i);
            StateChangeResult::Ok { state: groups_y }
        }
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
            groups_y.insert(group_id, members_y);
            StateChangeResult::Error {
                state: groups_y,
                error: err,
            }
        }
    }
}

/// Apply a remove operation without validating it against state change rules. This is required
/// when retaining mutual-remove operations which may have lost their delegated access rights.
pub(crate) fn apply_remove_unsafe<ID, C>(
    mut groups_y: GroupStates<ID, C>,
    group_id: ID,
    removed: GroupMember<ID>,
) -> GroupStates<ID, C>
where
    ID: IdentityHandle,
    C: Conditions,
{
    let mut members_y = groups_y
        .remove(&group_id)
        .expect("group already present in states map");

    members_y.members.entry(removed).and_modify(|state| {
        if state.member_counter % 2 != 0 {
            state.member_counter += 1
        }
    });
    groups_y.insert(group_id, members_y);
    groups_y
}

/// Return types expected from applying an action to group state.
pub enum StateChangeResult<ID, C>
where
    ID: IdentityHandle,
    C: Conditions,
{
    /// Action was applied and no error occurred.
    Ok { state: GroupStates<ID, C> },

    /// Action was not applied because it failed internal validation.
    Error {
        state: GroupStates<ID, C>,
        #[allow(unused)]
        error: GroupMembershipError<GroupMember<ID>>,
    },

    /// Action was not applied because it has been filtered out.
    Filtered { state: GroupStates<ID, C> },
}

impl<ID, C> StateChangeResult<ID, C>
where
    ID: IdentityHandle,
    C: Conditions,
{
    pub fn state(&self) -> &GroupStates<ID, C> {
        match self {
            StateChangeResult::Ok { state }
            | StateChangeResult::Error { state, .. }
            | StateChangeResult::Filtered { state } => state,
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use crate::Access;
    use crate::group::{GroupCrdtError, GroupMember, GroupMembershipError};
    use crate::test_utils::no_ord::{TestGroup, TestGroupState};
    use crate::test_utils::{
        add_member, create_group, demote_member, promote_member, remove_member,
    };
    use crate::traits::Operation;

    const G1: char = '1';
    const G2: char = '2';
    const G3: char = '3';
    const G4: char = '4';

    const ALICE: char = 'A';
    const BOB: char = 'B';
    const CLAIRE: char = 'C';
    const DAN: char = 'D';
    const EVE: char = 'E';

    #[test]
    fn group_operations() {
        let y = TestGroupState::new(());

        let op1 = create_group(
            ALICE,
            0,
            G1,
            vec![(GroupMember::Individual(ALICE), Access::manage())],
            vec![],
        );

        let y_i = TestGroup::process(y, &op1).unwrap();
        let mut members = y_i.members(G1);
        members.sort();
        assert_eq!(members, vec![(ALICE, Access::manage())]);

        let op2 = add_member(
            ALICE,
            1,
            G1,
            GroupMember::Individual(BOB),
            Access::read(),
            vec![op1.id()],
        );

        let y_ii = TestGroup::process(y_i, &op2).unwrap();
        let mut members = y_ii.members(G1);
        members.sort();
        assert_eq!(
            members,
            vec![(ALICE, Access::manage()), (BOB, Access::read())]
        );

        let op3 = add_member(
            ALICE,
            2,
            G1,
            GroupMember::Individual(CLAIRE),
            Access::write(),
            vec![op2.id()],
        );

        let y_iii = TestGroup::process(y_ii, &op3).unwrap();
        let mut members = y_iii.members(G1);
        members.sort();
        assert_eq!(
            members,
            vec![
                (ALICE, Access::manage()),
                (BOB, Access::read()),
                (CLAIRE, Access::write())
            ]
        );

        let op4 = remove_member(ALICE, 3, G1, GroupMember::Individual(BOB), vec![op3.id()]);

        let y_iv = TestGroup::process(y_iii, &op4).unwrap();
        let mut members = y_iv.members(G1);
        members.sort();
        assert_eq!(
            members,
            vec![(ALICE, Access::manage()), (CLAIRE, Access::write())]
        );
    }

    #[test]
    fn concurrent_removal() {
        let y = TestGroupState::new(());

        let op1 = create_group(
            ALICE,
            0,
            G1,
            vec![(GroupMember::Individual(ALICE), Access::manage())],
            vec![],
        );

        let y_i = TestGroup::process(y, &op1).unwrap();
        let mut members = y_i.members(G1);
        members.sort();
        assert_eq!(members, vec![(ALICE, Access::manage())]);

        let op2 = add_member(
            ALICE,
            1,
            G1,
            GroupMember::Individual(BOB),
            Access::manage(),
            vec![op1.id()],
        );

        let y_ii = TestGroup::process(y_i, &op2).unwrap();
        let mut members = y_ii.members(G1);
        members.sort();
        assert_eq!(
            members,
            vec![(ALICE, Access::manage()), (BOB, Access::manage())]
        );

        let op3 = add_member(
            BOB,
            2,
            G1,
            GroupMember::Individual(CLAIRE),
            Access::write(),
            vec![op2.id()],
        );

        let y_iii = TestGroup::process(y_ii, &op3).unwrap();
        let mut members = y_iii.members(G1);
        members.sort();
        assert_eq!(
            members,
            vec![
                (ALICE, Access::manage()),
                (BOB, Access::manage()),
                (CLAIRE, Access::write())
            ]
        );

        let op4 = remove_member(ALICE, 3, G1, GroupMember::Individual(BOB), vec![op2.id()]);

        let y_iv = TestGroup::process(y_iii, &op4).unwrap();
        let mut members = y_iv.members(G1);
        members.sort();
        assert_eq!(members, vec![(ALICE, Access::manage())]);
    }

    #[test]
    fn mutual_concurrent_removal() {
        let y = TestGroupState::new(());

        let op1 = create_group(
            ALICE,
            0,
            G1,
            vec![(GroupMember::Individual(ALICE), Access::manage())],
            vec![],
        );

        let y_i = TestGroup::process(y, &op1).unwrap();
        let mut members = y_i.members(G1);
        members.sort();
        assert_eq!(members, vec![(ALICE, Access::manage())]);

        let op2 = add_member(
            ALICE,
            1,
            G1,
            GroupMember::Individual(BOB),
            Access::manage(),
            vec![op1.id()],
        );

        let y_ii = TestGroup::process(y_i, &op2).unwrap();
        let mut members = y_ii.members(G1);
        members.sort();
        assert_eq!(
            members,
            vec![(ALICE, Access::manage()), (BOB, Access::manage())]
        );

        let op3 = add_member(
            BOB,
            2,
            G1,
            GroupMember::Individual(CLAIRE),
            Access::manage(),
            vec![op2.id()],
        );

        let y_iii = TestGroup::process(y_ii, &op3).unwrap();
        let mut members = y_iii.members(G1);
        members.sort();
        assert_eq!(
            members,
            vec![
                (ALICE, Access::manage()),
                (BOB, Access::manage()),
                (CLAIRE, Access::manage())
            ]
        );

        let op4 = remove_member(BOB, 3, G1, GroupMember::Individual(CLAIRE), vec![op3.id()]);

        let y_iv = TestGroup::process(y_iii, &op4).unwrap();
        let mut members = y_iv.members(G1);
        members.sort();
        assert_eq!(
            members,
            vec![(ALICE, Access::manage()), (BOB, Access::manage())]
        );

        let op5 = remove_member(CLAIRE, 4, G1, GroupMember::Individual(BOB), vec![op3.id()]);

        let y_v = TestGroup::process(y_iv, &op5).unwrap();
        let mut members = y_v.members(G1);
        members.sort();
        assert_eq!(members, vec![(ALICE, Access::manage())]);
    }

    #[test]
    fn nested_groups() {
        let y = TestGroupState::new(());

        let op1 = create_group(
            ALICE,
            0,
            G1,
            vec![(GroupMember::Individual(ALICE), Access::manage())],
            vec![],
        );

        let y_i = TestGroup::process(y, &op1).unwrap();
        let mut members = y_i.members(G1);
        members.sort();
        assert_eq!(members, vec![(ALICE, Access::manage())]);

        let op2 = create_group(
            BOB,
            1,
            G2,
            vec![(GroupMember::Individual(BOB), Access::manage())],
            vec![op1.id()],
        );

        let y_ii = TestGroup::process(y_i, &op2).unwrap();
        let mut members = y_ii.members(G2);
        members.sort();
        assert_eq!(members, vec![(BOB, Access::manage())]);

        let op3 = add_member(
            ALICE,
            2,
            G1,
            GroupMember::Group(G2),
            Access::read(),
            vec![op2.id()],
        );

        let y_iii = TestGroup::process(y_ii, &op3).unwrap();
        let mut members = y_iii.members(G1);
        members.sort();
        assert_eq!(
            members,
            vec![(ALICE, Access::manage()), (BOB, Access::read())]
        );
    }

    #[test]
    fn error_on_unauthorized_add() {
        let y = TestGroupState::new(());

        let op1 = create_group(
            ALICE,
            0,
            G1,
            vec![(GroupMember::Individual(ALICE), Access::manage())],
            vec![],
        );

        let y_i = TestGroup::process(y, &op1).unwrap();

        let op2 = add_member(
            ALICE,
            1,
            G1,
            GroupMember::Individual(BOB),
            Access::read(),
            vec![op1.id()],
        );

        let y_ii = TestGroup::process(y_i, &op2).unwrap();

        let op3 = add_member(
            BOB,
            2,
            G1,
            GroupMember::Individual(CLAIRE),
            Access::read(),
            vec![op2.id()],
        );

        assert!(TestGroup::process(y_ii, &op3).is_err());
    }

    #[test]
    fn error_on_remove_non_member() {
        let y = TestGroupState::new(());

        let op1 = create_group(
            ALICE,
            0,
            G1,
            vec![(GroupMember::Individual(ALICE), Access::manage())],
            vec![],
        );

        let y_i = TestGroup::process(y, &op1).unwrap();

        let op2 = remove_member(ALICE, 1, G1, GroupMember::Individual(BOB), vec![op1.id()]);

        assert!(TestGroup::process(y_i, &op2).is_err());
    }

    #[test]
    fn error_on_promote_non_member() {
        let y = TestGroupState::new(());

        let op1 = create_group(
            ALICE,
            0,
            G1,
            vec![(GroupMember::Individual(ALICE), Access::manage())],
            vec![],
        );

        let y_i = TestGroup::process(y, &op1).unwrap();

        let op2 = promote_member(
            ALICE,
            1,
            G1,
            GroupMember::Individual(BOB),
            Access::manage(),
            vec![op1.id()],
        );

        assert!(TestGroup::process(y_i, &op2).is_err());
    }

    #[test]
    fn error_on_add_manager_group() {
        let y = TestGroupState::new(());

        let op1 = create_group(
            ALICE,
            0,
            G1,
            vec![(GroupMember::Individual(ALICE), Access::manage())],
            vec![],
        );

        let y_i = TestGroup::process(y, &op1).unwrap();

        let op2 = add_member(
            ALICE,
            1,
            G1,
            GroupMember::Group(BOB),
            Access::manage(),
            vec![op1.id()],
        );

        assert!(TestGroup::process(y_i, &op2).is_err());
    }

    #[test]
    fn error_on_demote_non_member() {
        let y = TestGroupState::new(());

        let op1 = create_group(
            ALICE,
            0,
            G1,
            vec![(GroupMember::Individual(ALICE), Access::manage())],
            vec![],
        );

        let y_i = TestGroup::process(y, &op1).unwrap();

        let op2 = demote_member(
            ALICE,
            1,
            G1,
            GroupMember::Individual(BOB),
            Access::read(),
            vec![op1.id()],
        );

        assert!(TestGroup::process(y_i, &op2).is_err());
    }

    #[test]
    fn error_on_add_existing_member() {
        let y = TestGroupState::new(());

        let op1 = create_group(
            ALICE,
            0,
            G1,
            vec![(GroupMember::Individual(ALICE), Access::manage())],
            vec![],
        );

        let y_i = TestGroup::process(y, &op1).unwrap();

        let op2 = add_member(
            ALICE,
            1,
            G1,
            GroupMember::Individual(ALICE),
            Access::manage(),
            vec![op1.id()],
        );

        assert!(TestGroup::process(y_i, &op2).is_err());
    }

    #[test]
    fn error_on_remove_nonexistent_subgroup() {
        let y = TestGroupState::new(());

        let op1 = create_group(
            ALICE,
            0,
            G1,
            vec![(GroupMember::Individual(ALICE), Access::manage())],
            vec![],
        );
        let y_i = TestGroup::process(y, &op1).unwrap();

        // Attempt to remove a subgroup that was never added
        let op2 = remove_member(ALICE, 1, G1, GroupMember::Group(G2), vec![op1.id()]);

        assert!(TestGroup::process(y_i, &op2).is_err());
    }

    #[test]
    fn deeply_nested_groups_with_removals() {
        let y = TestGroupState::new(());

        // Create G1
        let op1 = create_group(
            ALICE,
            0,
            G1,
            vec![(GroupMember::Individual(ALICE), Access::manage())],
            vec![],
        );
        let y_i = TestGroup::process(y, &op1).unwrap();

        // Create G2
        let op2 = create_group(
            BOB,
            1,
            G2,
            vec![(GroupMember::Individual(BOB), Access::manage())],
            vec![op1.id()],
        );
        let y_ii = TestGroup::process(y_i, &op2).unwrap();

        // Create G3
        let op3 = create_group(
            CLAIRE,
            2,
            G3,
            vec![(GroupMember::Individual(CLAIRE), Access::manage())],
            vec![op2.id()],
        );
        let y_iii = TestGroup::process(y_ii, &op3).unwrap();

        // Create G4
        let op4 = create_group(
            DAN,
            3,
            G4,
            vec![(GroupMember::Individual(DAN), Access::write())],
            vec![op3.id()],
        );
        let y_iv = TestGroup::process(y_iii, &op4).unwrap();

        // Nest G4 into G3
        let op5 = add_member(
            CLAIRE,
            4,
            G3,
            GroupMember::Group(G4),
            Access::read(),
            vec![op4.id()],
        );
        let y_v = TestGroup::process(y_iv, &op5).unwrap();

        // Nest G3 into G2
        let op6 = add_member(
            BOB,
            5,
            G2,
            GroupMember::Group(G3),
            Access::write(),
            vec![op5.id()],
        );
        let y_vi = TestGroup::process(y_v, &op6).unwrap();

        // Nest G2 into G1
        let op7 = add_member(
            ALICE,
            6,
            G1,
            GroupMember::Group(G2),
            Access::read(),
            vec![op6.id()],
        );
        let y_vii = TestGroup::process(y_vi, &op7).unwrap();

        let mut members = y_vii.members(G1);
        members.sort();
        assert_eq!(
            members,
            vec![
                (ALICE, Access::manage()),
                (BOB, Access::read()),
                (CLAIRE, Access::read()),
                (DAN, Access::read()),
            ]
        );

        // Remove G3 from G2
        let op8 = remove_member(BOB, 7, G2, GroupMember::Group(G3), vec![op7.id()]);
        let y_viii = TestGroup::process(y_vii, &op8).unwrap();

        let mut members_after_removal = y_viii.members(G1);
        members_after_removal.sort();
        assert_eq!(
            members_after_removal,
            vec![(ALICE, Access::manage()), (BOB, Access::read()),]
        );
    }

    #[test]
    fn nested_groups_with_concurrent_removal_and_promotion() {
        let y = TestGroupState::new(());

        // Create G1
        let op1 = create_group(
            ALICE,
            0,
            G1,
            vec![(GroupMember::Individual(ALICE), Access::manage())],
            vec![],
        );
        let y_i = TestGroup::process(y, &op1).unwrap();

        // Create G2
        let op2 = create_group(
            BOB,
            1,
            G2,
            vec![(GroupMember::Individual(BOB), Access::manage())],
            vec![op1.id()],
        );
        let y_ii = TestGroup::process(y_i, &op2).unwrap();

        // Create G3
        let op3 = create_group(
            CLAIRE,
            2,
            G3,
            vec![(GroupMember::Individual(CLAIRE), Access::manage())],
            vec![op2.id()],
        );
        let y_iii = TestGroup::process(y_ii, &op3).unwrap();

        // G3 includes Dan
        let op4 = add_member(
            CLAIRE,
            3,
            G3,
            GroupMember::Individual(DAN),
            Access::write(),
            vec![op3.id()],
        );
        let y_iv = TestGroup::process(y_iii, &op4).unwrap();

        // G2 includes G3
        let op5 = add_member(
            BOB,
            4,
            G2,
            GroupMember::Group(G3),
            Access::write(),
            vec![op4.id()],
        );
        let y_v = TestGroup::process(y_iv, &op5).unwrap();

        // G2 includes Claire
        let op6 = add_member(
            BOB,
            5,
            G2,
            GroupMember::Individual(CLAIRE),
            Access::read(),
            vec![op5.id()],
        );
        let y_vi = TestGroup::process(y_v, &op6).unwrap();

        // G1 includes G2
        let op7 = add_member(
            ALICE,
            6,
            G1,
            GroupMember::Group(G2),
            Access::read(),
            vec![op6.id()],
        );
        let y_vii = TestGroup::process(y_vi, &op7).unwrap();

        let mut members = y_vii.members(G1);
        members.sort();
        assert_eq!(
            members,
            vec![
                (ALICE, Access::manage()),
                (BOB, Access::read()),
                (CLAIRE, Access::read()),
                (DAN, Access::read()),
            ]
        );

        // Concurrent ops from same parent state
        let op8_remove_g2 = remove_member(ALICE, 7, G1, GroupMember::Group(G2), vec![op7.id()]);
        let op9_promote_claire = promote_member(
            BOB,
            8,
            G2,
            GroupMember::Individual(CLAIRE),
            Access::manage(),
            vec![op7.id()],
        );

        // Remove first
        let y_after_remove = TestGroup::process(y_vii.clone(), &op8_remove_g2).unwrap();
        let mut members = y_after_remove.members(G1);
        members.sort();
        assert_eq!(members, vec![(ALICE, Access::manage())]);

        // Then promote
        let y_after_both = TestGroup::process(y_after_remove, &op9_promote_claire).unwrap();
        let mut g1_members = y_after_both.members(G1);
        g1_members.sort();
        assert_eq!(g1_members, vec![(ALICE, Access::manage())]);

        let mut g2_members = y_after_both.members(G2);
        g2_members.sort();
        assert_eq!(
            g2_members,
            vec![
                (BOB, Access::manage()),
                (CLAIRE, Access::manage()),
                (DAN, Access::write()),
            ]
        );
    }

    #[test]
    fn concurrent_removal_ooo_processing() {
        let y = TestGroupState::new(());

        // Alice creates group
        let op1 = create_group(
            ALICE,
            0,
            G1,
            vec![(GroupMember::Individual(ALICE), Access::manage())],
            vec![],
        );
        let y_i = TestGroup::process(y, &op1).unwrap();

        // Alice adds Bob as manager
        let op2 = add_member(
            ALICE,
            1,
            G1,
            GroupMember::Individual(BOB),
            Access::manage(),
            vec![op1.id()],
        );
        let y_ii = TestGroup::process(y_i, &op2).unwrap();

        // Bob adds Claire (Read)
        let op3 = add_member(
            BOB,
            2,
            G1,
            GroupMember::Individual(CLAIRE),
            Access::read(),
            vec![op2.id()],
        );

        // Alice removes Bob
        let op4 = remove_member(ALICE, 3, G1, GroupMember::Individual(BOB), vec![op2.id()]);

        // Apply in Order A: Add Claire, then Remove Bob
        let y_iii_a = TestGroup::process(y_ii.clone(), &op3).unwrap();
        let y_iv_a = TestGroup::process(y_iii_a, &op4).unwrap();

        // Apply in Order B: Remove Bob, then Add Claire
        let y_iii_b = TestGroup::process(y_ii.clone(), &op4).unwrap();
        let y_iv_b = TestGroup::process(y_iii_b, &op3).unwrap();

        for (_, y) in [y_iv_a, y_iv_b].into_iter().enumerate() {
            let mut members = y.members(G1);
            members.sort();
            assert_eq!(members, vec![(ALICE, Access::manage())],);
        }
    }

    #[test]
    fn concurrent_add_with_insufficient_access() {
        let y0 = TestGroupState::new(());

        // Alice creates the group
        let op1 = create_group(
            ALICE,
            0,
            G1,
            vec![(GroupMember::Individual(ALICE), Access::manage())],
            vec![],
        );
        let y1 = TestGroup::process(y0, &op1).unwrap();

        // Alice adds Bob as manager
        let op2 = add_member(
            ALICE,
            1,
            G1,
            GroupMember::Individual(BOB),
            Access::manage(),
            vec![op1.id()],
        );

        // Bob concurrently tries to add Eve
        let op3 = add_member(
            BOB,
            2,
            G1,
            GroupMember::Individual(EVE),
            Access::read(),
            vec![op1.id()],
        );

        // Case 1: Apply Bob's operation first - should fail
        let result = TestGroup::process(y1.clone(), &op3);
        assert!(matches!(
            result,
            Err(GroupCrdtError::StateChangeError(
                _,
                GroupMembershipError::UnrecognisedActor(_)
            ))
        ));

        // Case 2: Apply Alice’s op first, then Bob's - still must fail
        let y1_alt = TestGroup::process(y1, &op2).unwrap();
        let result = TestGroup::process(y1_alt.clone(), &op3);
        assert!(matches!(
            result,
            Err(GroupCrdtError::StateChangeError(
                _,
                GroupMembershipError::UnrecognisedActor(_)
            ))
        ));

        // Confirm final state: Bob is a member, Eve is not
        let mut members = y1_alt.members(G1);
        members.sort();
        assert_eq!(
            members,
            vec![(ALICE, Access::manage()), (BOB, Access::manage())]
        );
    }

    #[test]
    fn add_group_with_concurrent_change() {
        let y = TestGroupState::new(());

        // Create Group 1 with Alice as manager
        let op1 = create_group(
            ALICE,
            0,
            G1,
            vec![(GroupMember::Individual(ALICE), Access::manage())],
            vec![],
        );
        let y_i = TestGroup::process(y, &op1).unwrap();

        // Create Group 2 with Bob as manager
        let op2 = create_group(
            BOB,
            1,
            G2,
            vec![(GroupMember::Individual(BOB), Access::manage())],
            vec![op1.id()],
        );
        let y_ii = TestGroup::process(y_i, &op2).unwrap();

        // Alice adds Group 2 to Group 1
        let op3a = add_member(
            ALICE,
            2,
            G1,
            GroupMember::Group(G2),
            Access::read(),
            vec![op2.id()],
        );

        // Concurrently, Bob adds Claire to Group 2
        let op3b = add_member(
            BOB,
            3,
            G2,
            GroupMember::Individual(CLAIRE),
            Access::write(),
            vec![op2.id()],
        );

        // Order 1: Add group, then add member
        let y_iii = TestGroup::process(y_ii.clone(), &op3a).unwrap();
        let y_iv = TestGroup::process(y_iii, &op3b).unwrap();

        let mut members_1 = y_iv.members(G1);
        members_1.sort();
        assert_eq!(
            members_1,
            vec![
                (ALICE, Access::manage()),
                (BOB, Access::read()),
                (CLAIRE, Access::read())
            ]
        );

        // Order 2: Add member, then add group
        let y_iii_alt = TestGroup::process(y_ii.clone(), &op3b).unwrap();
        let y_iv_alt = TestGroup::process(y_iii_alt, &op3a).unwrap();

        let mut members_1 = y_iv_alt.members(G1);
        members_1.sort();
        assert_eq!(
            members_1,
            vec![
                (ALICE, Access::manage()),
                (BOB, Access::read()),
                (CLAIRE, Access::read())
            ]
        );
    }

    #[test]
    fn nested_group_cycle_error() {
        let y = TestGroupState::new(());

        // Create group G1 with ALICE as manager
        let op1 = create_group(
            ALICE,
            0,
            G1,
            vec![(GroupMember::Individual(ALICE), Access::manage())],
            vec![],
        );
        let y_i = TestGroup::process(y, &op1).unwrap();

        // Create group G2 with BOB as manager, with G1 as a member
        let op2 = create_group(
            BOB,
            1,
            G2,
            vec![
                (GroupMember::Individual(BOB), Access::manage()),
                (GroupMember::Group(G1), Access::read()),
            ],
            vec![op1.id()],
        );
        let y_ii = TestGroup::process(y_i, &op2).unwrap();

        // Attempt to add G2 as a member of G1, which creates a cycle (G1 -> G2 -> G1)
        let op3 = add_member(
            ALICE,
            2,
            G1,
            GroupMember::Group(G2),
            Access::read(),
            vec![op2.id()],
        );

        // This should fail due to cycle detection
        let result = TestGroup::process(y_ii, &op3);
        assert!(
            result.is_err(),
            "Creating a group cycle should cause an error"
        );
    }
}
