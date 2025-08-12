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

/// Error types for GroupCrdt.
#[derive(Debug, Error)]
pub enum GroupCrdtError<ID, OP, C, RS, ORD>
where
    ID: IdentityHandle,
    OP: OperationId + Ord,
    RS: Resolver<ID, OP, C, ORD::Operation>,
    ORD: Orderer<ID, OP, GroupControlMessage<ID, C>> + Debug,
{
    #[error("duplicate operation {0} processed in group {1}")]
    DuplicateOperation(OP, ID),

    #[error("state change error processing operation {0}: {1:?}")]
    StateChangeError(OP, GroupMembershipError<GroupMember<ID>>),

    #[error("expected sub-group {0} to exist in the store")]
    MissingSubGroup(ID),

    #[error("attempted to add group {0} with manage access")]
    ManagerGroupsNotAllowed(ID),

    #[error("ordering error: {0}")]
    Orderer(ORD::Error),

    #[error("ordering error: {0}")]
    Resolver(RS::Error),

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

#[derive(Debug)]
#[cfg_attr(any(test, feature = "test_utils"), derive(Clone))]
pub struct AuthState<ID, OP, C, M>
where
    ID: IdentityHandle,
    OP: OperationId + Ord,
{
    /// All operations processed by this group.
    pub operations: HashMap<OP, M>,

    /// All operations who's actions should be ignored.
    pub ignore: HashSet<OP>,

    /// States for this auth group.
    pub states: HashMap<OP, HashMap<ID, GroupMembersState<GroupMember<ID>, C>>>,

    /// Operation graph for all auth groups.
    pub graph: DiGraphMap<OP, ()>,
}

impl<ID, OP, C, M> Default for AuthState<ID, OP, C, M>
where
    ID: IdentityHandle,
    OP: OperationId + Ord,
{
    fn default() -> Self {
        Self {
            operations: Default::default(),
            ignore: Default::default(),
            states: Default::default(),
            graph: Default::default(),
        }
    }
}

impl<ID, OP, C, M> AuthState<ID, OP, C, M>
where
    ID: IdentityHandle,
    OP: OperationId + Ord,
    C: Conditions,
{
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

    /// Current state of this group.
    ///
    /// This method gets the state at all graph tips and then merges them together into one new
    /// state which represents the current state of the group.
    pub fn current_state(&self) -> HashMap<ID, GroupMembersState<GroupMember<ID>, C>> {
        self.merge_states(&self.heads())
    }

    pub(crate) fn state_at(
        &self,
        dependencies: &HashSet<OP>,
    ) -> HashMap<ID, GroupMembersState<GroupMember<ID>, C>> {
        self.merge_states(dependencies)
    }

    fn merge_states(
        &self,
        ids: &HashSet<OP>,
    ) -> HashMap<ID, GroupMembersState<GroupMember<ID>, C>> {
        let mut current_state = HashMap::new();
        for id in ids {
            // Unwrap as this method is only used internally where all requested states should exist.
            let group_states = self.states.get(id).unwrap().clone();
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
        current_state
    }

    fn members_inner(
        &self,
        group_id: ID,
        members: &mut HashMap<ID, Access<C>>,
        root_access: Option<Access<C>>,
    ) {
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
                GroupMember::Group(id) => self.members_inner(id, members, Some(next_access)),
            }
        }
    }

    /// Get all current members of the group.
    pub fn members(&self, group_id: ID) -> Vec<(ID, Access<C>)> {
        let mut members = HashMap::new();
        self.members_inner(group_id, &mut members, None);
        members.into_iter().collect()
    }
}

#[derive(Debug)]
#[cfg_attr(any(test, feature = "test_utils"), derive(Clone))]
pub struct GroupCrdtState<ID, OP, C, ORD>
where
    ID: IdentityHandle,
    OP: OperationId + Ord,
    ORD: Orderer<ID, OP, GroupControlMessage<ID, C>> + Debug,
    ORD::Operation: Clone,
{
    pub auth_y: AuthState<ID, OP, C, ORD::Operation>,

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
    /// Instantiate a new group state.
    pub fn new(orderer_y: ORD::State) -> Self {
        Self {
            auth_y: AuthState::default(),
            orderer_y,
        }
    }

    /// Get all current members of the group.
    pub fn root_members(&self, group_id: ID) -> Vec<(GroupMember<ID>, Access<C>)> {
        match self.auth_y.current_state().get(&group_id) {
            Some(group_y) => group_y.access_levels(),
            None => vec![],
        }
    }

    /// Get all current members of the group.
    pub fn members(&self, group_id: ID) -> Vec<(ID, Access<C>)> {
        self.auth_y.members(group_id)
    }
}

#[derive(Clone, Debug, Default)]
pub struct GroupCrdt<ID, OP, C, RS, ORD> {
    _phantom: PhantomData<(ID, OP, C, RS, ORD)>,
}

impl<ID, OP, C, RS, ORD> GroupCrdt<ID, OP, C, RS, ORD>
where
    ID: IdentityHandle,
    OP: OperationId + Ord,
    C: Conditions,
    RS: Resolver<ID, OP, C, ORD::Operation, State = AuthState<ID, OP, C, ORD::Operation>>,
    ORD: Orderer<ID, OP, GroupControlMessage<ID, C>> + Debug,
    ORD::Operation: Clone,
{
    pub fn init(orderer_y: ORD::State) -> GroupCrdtState<ID, OP, C, ORD> {
        GroupCrdtState {
            auth_y: AuthState::default(),
            orderer_y,
        }
    }

    /// Prepare a next operation to be processed locally and sent to remote peers. An ORD
    /// implementation needs to ensure "previous" and "dependencies" are populated correctly so
    /// that a partial-order of all operations in the system can be established.
    ///
    /// The method `GroupCrdtState::heads` and `GroupCrdtState::transitive_heads` can be used to retrieve the
    /// operation ids of these operation dependencies.
    pub fn prepare(
        mut y: GroupCrdtState<ID, OP, C, ORD>,
        action: &GroupControlMessage<ID, C>,
    ) -> Result<(GroupCrdtState<ID, OP, C, ORD>, ORD::Operation), GroupCrdtError<ID, OP, C, RS, ORD>>
    {
        // Get the next operation from our global orderer. The operation wraps the action we want
        // to perform, adding ordering and author meta-data.
        let ordering_y = y.orderer_y;
        let (ordering_y, operation) =
            ORD::next_message(ordering_y, action).map_err(GroupCrdtError::Orderer)?;
        y.orderer_y = ordering_y;
        Ok((y, operation))
    }

    /// Process an operation created locally or received from a remote peer.
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
            RS::rebuild_required(&y.auth_y, &operation).map_err(GroupCrdtError::Resolver)?;

        // Adding a group as a manager of another group is currently not
        // supported.
        //
        // @TODO: To support this behavior updates in the StrongRemove resolver
        // so that cross-group concurrent remove cycles are detected. Related to
        // issue: https://github.com/p2panda/p2panda/issues/779 
        match &control_message.action {
            GroupAction::Add { member, access } | GroupAction::Promote { member, access } => {
                if member.is_group() && access.is_manage() {
                    return Err(GroupCrdtError::ManagerGroupsNotAllowed(member.id()));
                }
            }
            _ => (),
        };

        // Validate that the author of this operation had the required access
        // rights at the point in the auth graph which they claim as their last
        // state (the state at "dependencies"). It could be that they had access
        // at this point but concurrent changes (which we know about) mean that
        // they have lost that access level. This case is dealt with later, here
        // we want to catch malicious operations which should _never_ be
        // attached to the graph.
        if rebuild_required {
            y = GroupCrdt::authorize(y, operation)?;
        }

        // Add operation to the global auth graph.
        y.auth_y.graph.add_node(operation_id);
        for dependency in &dependencies {
            y.auth_y.graph.add_edge(*dependency, operation_id, ());
        }

        // Insert operation into all operations map.
        y.auth_y.operations.insert(operation_id, operation.clone());

        if rebuild_required {
            y.auth_y = RS::process(y.auth_y).map_err(GroupCrdtError::Resolver)?;
            return Ok(y);
        }

        let mut groups_y = y.auth_y.state_at(&dependencies);
        let result = apply_action(
            groups_y,
            group_id,
            operation_id,
            actor,
            &control_message.action,
            &y.auth_y.ignore,
        );

        groups_y = match result {
            StateChangeResult::Ok { state } => state,
            StateChangeResult::Noop { error, .. } => {
                // Noop shouldn't happen when processing new operations as the
                // rebuild logic should have occurred instead.
                return Err(GroupCrdtError::StateChangeError(operation_id, error));
            }
            StateChangeResult::Filtered { .. } => {
                // Operations can't be filtered out before they were processed.
                unreachable!();
            }
        };

        y.auth_y.states.insert(operation_id, groups_y);

        Ok(y)
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
    pub(crate) fn authorize(
        y: GroupCrdtState<ID, OP, C, ORD>,
        operation: &ORD::Operation,
    ) -> Result<GroupCrdtState<ID, OP, C, ORD>, GroupCrdtError<ID, OP, C, RS, ORD>> {
        // Keep hold of original operations and graph.
        let last_graph = y.auth_y.graph.clone();
        let last_ignore = y.auth_y.ignore.clone();
        let last_states = y.auth_y.states.clone();

        let mut temp_y = y;

        // Collect predecessors of the new operation.
        let mut predecessors = HashSet::new();
        for dependency in operation.dependencies() {
            let reversed = Reversed(&temp_y.auth_y.graph);
            let mut dfs_rev = DfsPostOrder::new(&reversed, dependency);
            while let Some(id) = dfs_rev.next(&reversed) {
                predecessors.insert(id);
            }
        }

        // Remove all other nodes from the graph.
        let to_remove: Vec<_> = temp_y
            .auth_y
            .graph
            .node_identifiers()
            .filter(|n| !predecessors.contains(n))
            .collect();

        for node in &to_remove {
            temp_y.auth_y.graph.remove_node(*node);
        }

        let temp_y_i = {
            temp_y.auth_y = RS::process(temp_y.auth_y).map_err(GroupCrdtError::Resolver)?;
            temp_y
        };

        let dependencies = HashSet::from_iter(operation.dependencies().clone());

        let groups_y = temp_y_i.auth_y.state_at(&dependencies);
        let result = apply_action(
            groups_y,
            operation.payload().group_id(),
            operation.id(),
            operation.author(),
            &operation.payload().action,
            &temp_y_i.auth_y.ignore,
        );

        match result {
            StateChangeResult::Ok { state } => state,
            StateChangeResult::Noop { error, .. } => {
                // Noop shouldn't happen when processing new operations as the
                // rebuild logic should have occurred instead.
                return Err(GroupCrdtError::StateChangeError(operation.id(), error));
            }
            StateChangeResult::Filtered { .. } => {
                // Operations can't be filtered out before they were processed.
                unreachable!();
            }
        };

        let mut y = temp_y_i;
        y.auth_y.graph = last_graph;
        y.auth_y.ignore = last_ignore;
        y.auth_y.states = last_states;

        Ok(y)
    }
}

/// Apply an action to a single group state.
#[allow(clippy::type_complexity)]
pub(crate) fn apply_action<ID, OP, C>(
    mut groups_y: HashMap<ID, GroupMembersState<GroupMember<ID>, C>>,
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
            return StateChangeResult::Ok { state: groups_y };
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
            return StateChangeResult::Noop {
                state: groups_y,
                error: err,
            };
        }
    };
}

/// Return types expected from applying an action to group state.
pub enum StateChangeResult<ID, C>
where
    ID: IdentityHandle,
    C: Conditions,
{
    /// Action was applied and no error occurred.
    Ok {
        state: HashMap<ID, GroupMembersState<GroupMember<ID>, C>>,
    },

    /// Action was not applied because it failed internal validation.
    Noop {
        state: HashMap<ID, GroupMembersState<GroupMember<ID>, C>>,
        #[allow(unused)]
        error: GroupMembershipError<GroupMember<ID>>,
    },

    /// Action was not applied because it has been filtered out.
    Filtered {
        state: HashMap<ID, GroupMembersState<GroupMember<ID>, C>>,
    },
}

impl<ID, C> StateChangeResult<ID, C>
where
    ID: IdentityHandle,
    C: Conditions,
{
    pub fn state(&self) -> &HashMap<ID, GroupMembersState<GroupMember<ID>, C>> {
        match self {
            StateChangeResult::Ok { state }
            | StateChangeResult::Noop { state, .. }
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
}
