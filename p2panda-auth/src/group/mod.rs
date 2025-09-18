// SPDX-License-Identifier: MIT OR Apache-2.0

//! Group membership and authorisation.

mod action;
mod authority_graphs;
pub(crate) mod crdt;
#[cfg(any(test, feature = "test_utils"))]
mod display;
mod member;
mod message;
pub mod resolver;

pub use action::GroupAction;
pub(crate) use authority_graphs::AuthorityGraphs;
pub(crate) use crdt::apply_action;
pub use crdt::state::{GroupMembersState, GroupMembershipError, MemberState};
pub use crdt::{
    GroupCrdt, GroupCrdtError, GroupCrdtInnerError, GroupCrdtInnerState, GroupCrdtState,
    StateChangeResult,
};
pub use member::GroupMember;
pub use message::GroupControlMessage;

use std::collections::HashSet;
use std::fmt::Debug;
use std::marker::PhantomData;

use thiserror::Error;

use crate::Access;
use crate::traits::{
    Conditions, GroupMembership, Groups as GroupsTrait, IdentityHandle, OperationId, Orderer,
    Resolver,
};

#[derive(Debug, Error)]
/// All possible errors that can occur when creating or updating a group.
pub enum GroupsError<ID, OP, C, RS, ORD>
where
    ID: IdentityHandle,
    OP: OperationId + Ord,
    RS: Resolver<ID, OP, C, ORD::Operation>,
    ORD: Orderer<ID, OP, GroupControlMessage<ID, C>> + Debug,
{
    #[error(transparent)]
    Group(#[from] GroupCrdtError<ID, OP, C, RS, ORD>),

    #[error("group must be created with at least one initial member")]
    EmptyGroup,

    #[error("actor {0} is already a member of group {1}")]
    GroupMember(ID, ID),

    #[error("actor {0} is not a member of group {1}")]
    NotGroupMember(ID, ID),

    #[error("action requires manager access but actor {0} is {1} in group {2}")]
    InsufficientAccess(ID, Access<C>, ID),

    #[error("actor {0} already has access level {1} in group {2}")]
    SameAccessLevel(ID, Access<C>, ID),

    #[error("state not found for group member {0} in group {1}")]
    MemberNotFound(ID, ID),
}

/// Decentralised Group Management (DGM).
///
/// The `Groups` provides a high-level interface for creating and updating groups. These groups
/// provide a means for restricting access to application data and resources. Groups are
/// comprised of members, which may be individuals or groups, and are assigned a user-chosen
/// identity. Each member is assigned a unique user-chosen identifier and access level. Access
/// levels are used to enforce restrictions over access to data and the mutation of that data.
/// They are also used to grant permissions which allow for mutating the group state by adding,
/// removing and modifying the access level of other members.
///
/// Each `Groups` method performs internal validation to ensure that the desired group action is
/// valid in light of the current group state. Attempting to perform an invalid action results in a
/// `GroupsError`. For example, attempting to remove a member who is not currently part of the
/// group.
pub struct Groups<ID, OP, C, RS, ORD>
where
    ID: IdentityHandle,
    OP: OperationId + Ord,
    C: Conditions,
    RS: Resolver<ID, OP, C, ORD::Operation, State = GroupCrdtInnerState<ID, OP, C, ORD::Operation>>
        + Debug,
    ORD: Orderer<ID, OP, GroupControlMessage<ID, C>> + Debug,
    ORD::Operation: Clone,
{
    my_id: ID,
    y: Option<GroupCrdtState<ID, OP, C, ORD>>,
    _phantom: PhantomData<RS>,
}

impl<ID, OP, C, RS, ORD> Groups<ID, OP, C, RS, ORD>
where
    ID: IdentityHandle,
    OP: OperationId + Ord,
    C: Conditions,
    RS: Resolver<ID, OP, C, ORD::Operation, State = GroupCrdtInnerState<ID, OP, C, ORD::Operation>>
        + Debug,
    ORD: Orderer<ID, OP, GroupControlMessage<ID, C>> + Debug,
    ORD::Operation: Clone,
{
    /// Initialise the `Group` state so that groups can be created and updated.
    ///
    /// Requires the identifier of the local actor, as well as a group store and orderer.
    pub fn new(my_id: ID, y: GroupCrdtState<ID, OP, C, ORD>) -> Self {
        Self {
            my_id,
            y: Some(y),
            _phantom: PhantomData,
        }
    }

    /// Take the current state from the groups struct consuming self in the process.
    pub fn take_state(mut self) -> GroupCrdtState<ID, OP, C, ORD> {
        self.y.take().expect("state object present")
    }
}

impl<ID, OP, C, RS, ORD> GroupsTrait<ID, OP, C, ORD::Operation> for Groups<ID, OP, C, RS, ORD>
where
    ID: IdentityHandle,
    OP: OperationId + Ord,
    C: Conditions,
    RS: Resolver<ID, OP, C, ORD::Operation, State = GroupCrdtInnerState<ID, OP, C, ORD::Operation>>
        + Debug,
    ORD: Orderer<ID, OP, GroupControlMessage<ID, C>> + Debug,
    ORD::Operation: Clone,
{
    type Error = GroupsError<ID, OP, C, RS, ORD>;

    /// Create a group.
    ///
    /// The creator of the group is automatically added as a manager.
    ///
    /// The caller of this method must ensure that the given `group_id` is globally unique. For
    /// example, using a collision-resistant hash.
    fn create(
        &mut self,
        group_id: ID,
        members: Vec<(GroupMember<ID>, Access<C>)>,
    ) -> Result<ORD::Operation, Self::Error> {
        // The creator of the group is automatically added as a manager.
        let creator = (GroupMember::Individual(self.my_id), Access::manage());

        let mut initial_members = Vec::new();
        initial_members.push(creator);
        initial_members.extend(members);

        let action = GroupControlMessage {
            group_id,
            action: GroupAction::Create { initial_members },
        };

        let y = self.y.take().expect("state object present");
        let (y_i, operation) = GroupCrdt::prepare(y, &action)?;
        let y_ii = GroupCrdt::process(y_i, &operation)?;
        let _ = self.y.insert(y_ii);

        Ok(operation)
    }

    /// Update a group by processing a remotely-authored action.
    ///
    /// The `group_id` of the given operation must be the same as that of the given `y`; failure to
    /// meet this condition will result in an error.
    fn receive_from_remote(&mut self, remote_operation: ORD::Operation) -> Result<(), Self::Error> {
        // Validation is performed internally by `process()`.
        let y = self.y.take().expect("state object present");
        let y_i = GroupCrdt::process(y, &remote_operation)?;
        let _ = self.y.insert(y_i);

        Ok(())
    }

    /// Add a group member.
    ///
    /// The `adder` must be a manager and the `added` identity must not already be a member of
    /// the group; failure to meet these conditions will result in an error.
    fn add(
        &mut self,
        group_id: ID,
        adder: ID,
        added: ID,
        access: Access<C>,
    ) -> Result<ORD::Operation, Self::Error> {
        if !self.is_manager(group_id, adder)? {
            let adder_access = self.access(group_id, adder)?;
            return Err(GroupsError::InsufficientAccess(
                adder,
                adder_access,
                group_id,
            ));
        }

        if self.is_member(group_id, added)? {
            return Err(GroupsError::GroupMember(added, group_id));
        }

        let action = GroupControlMessage {
            group_id,
            action: GroupAction::Add {
                member: GroupMember::Individual(added),
                access,
            },
        };

        let y = self.y.take().expect("state object present");
        let (y_i, operation) = GroupCrdt::prepare(y, &action)?;
        let y_ii = GroupCrdt::process(y_i, &operation)?;
        let _ = self.y.insert(y_ii);

        Ok(operation)
    }

    /// Remove a group member.
    ///
    /// The `remover` must be a manager and the `removed` identity must already be a member
    /// of the group; failure to meet these conditions will result in an error. A member can only
    /// remove themself from the group if they are a manager.
    // TODO: Consider introducing self-removal for non-manager members:
    //
    // https://github.com/p2panda/p2panda/issues/759
    fn remove(
        &mut self,
        group_id: ID,
        remover: ID,
        removed: ID,
    ) -> Result<ORD::Operation, Self::Error> {
        if !self.is_manager(group_id, remover)? {
            let remover_access = self.access(group_id, remover)?;
            return Err(GroupsError::InsufficientAccess(
                remover,
                remover_access,
                group_id,
            ));
        }

        if !self.is_member(group_id, removed)? {
            return Err(GroupsError::NotGroupMember(removed, group_id));
        }

        let action = GroupControlMessage {
            group_id,
            action: GroupAction::Remove {
                member: GroupMember::Individual(removed),
            },
        };

        let y = self.y.take().expect("state object present");
        let (y_i, operation) = GroupCrdt::prepare(y, &action)?;
        let y_ii = GroupCrdt::process(y_i, &operation)?;
        let _ = self.y.insert(y_ii);

        Ok(operation)
    }

    /// Promote a group member to the given access level.
    ///
    /// The `promoter` must be a manager and the `promoted` identity must already be a member of
    /// the group; failure to meet these conditions will result in an error. A redundant access
    /// level assignment will also result in an error; for example, if the `promoted` member
    /// currently has `Read` access and the given access is also `Read`.
    fn promote(
        &mut self,
        group_id: ID,
        promoter: ID,
        promoted: ID,
        access: Access<C>,
    ) -> Result<ORD::Operation, Self::Error> {
        if !self.is_manager(group_id, promoter)? {
            let promoter_access = self.access(group_id, promoter)?;
            return Err(GroupsError::InsufficientAccess(
                promoter,
                promoter_access,
                group_id,
            ));
        }

        if !self.is_member(group_id, promoted)? {
            return Err(GroupsError::NotGroupMember(promoted, group_id));
        }

        // Prevent redundant access level assignment.
        if self.access(group_id, promoted)? == access {
            return Err(GroupsError::SameAccessLevel(promoted, access, group_id));
        }

        let action = GroupControlMessage {
            group_id,
            action: GroupAction::Promote {
                member: GroupMember::Individual(promoted),
                access,
            },
        };

        let y = self.y.take().expect("state object present");
        let (y_i, operation) = GroupCrdt::prepare(y, &action)?;
        let y_ii = GroupCrdt::process(y_i, &operation)?;
        let _ = self.y.insert(y_ii);

        Ok(operation)
    }

    /// Demote a group member to the given access level.
    ///
    /// The `demoter` must be a manager and the `demoted` identity must already be a member of
    /// the group; failure to meet these conditions will result in an error. A redundant access
    /// level assignment will also result in an error; for example, if the `demoted` member
    /// currently has `Manage` access and the given access is also `Manage`.
    fn demote(
        &mut self,
        group_id: ID,
        demoter: ID,
        demoted: ID,
        access: Access<C>,
    ) -> Result<ORD::Operation, Self::Error> {
        if !self.is_manager(group_id, demoter)? {
            let demoter_access = self.access(group_id, demoter)?;
            return Err(GroupsError::InsufficientAccess(
                demoter,
                demoter_access,
                group_id,
            ));
        }

        if !self.is_member(group_id, demoted)? {
            return Err(GroupsError::NotGroupMember(demoted, group_id));
        }

        // Prevent redundant access level assignment.
        if self.access(group_id, demoted)? == access {
            return Err(GroupsError::SameAccessLevel(demoted, access, group_id));
        }

        let action = GroupControlMessage {
            group_id,
            action: GroupAction::Demote {
                member: GroupMember::Individual(demoted),
                access,
            },
        };

        let y = self.y.take().expect("state object present");
        let (y_i, operation) = GroupCrdt::prepare(y, &action)?;
        let y_ii = GroupCrdt::process(y_i, &operation)?;
        let _ = self.y.insert(y_ii);

        Ok(operation)
    }
}

impl<ID, OP, C, RS, ORD> GroupMembership<ID, OP, C> for Groups<ID, OP, C, RS, ORD>
where
    ID: IdentityHandle,
    OP: OperationId + Ord,
    C: Conditions,
    RS: Resolver<ID, OP, C, ORD::Operation, State = GroupCrdtInnerState<ID, OP, C, ORD::Operation>>
        + Debug,
    ORD: Orderer<ID, OP, GroupControlMessage<ID, C>> + Debug,
    ORD::Operation: Clone,
{
    type Error = GroupsError<ID, OP, C, RS, ORD>;

    /// Query the current access level of the given member.
    ///
    /// The member is expected to be a "stateless" individual, not a "stateful" group.
    fn access(&self, group_id: ID, member: ID) -> Result<Access<C>, Self::Error> {
        let Some(y) = &self.y else { unreachable!() };

        let member_state = y
            .members(group_id)
            .into_iter()
            .find(|(member_id, _state)| member_id == &member);

        if let Some(state) = member_state {
            let access = state.1.to_owned();

            Ok(access)
        } else {
            Err(GroupsError::MemberNotFound(group_id, member))
        }
    }

    /// Query group membership.
    fn member_ids(&self, group_id: ID) -> Result<HashSet<ID>, Self::Error> {
        let Some(y) = &self.y else { unreachable!() };

        let member_ids = y
            .members(group_id)
            .into_iter()
            .map(|(member_id, _state)| member_id)
            .collect();

        Ok(member_ids)
    }

    /// Return `true` if the given ID is an active member of the group.
    fn is_member(&self, group_id: ID, member: ID) -> Result<bool, Self::Error> {
        let Some(y) = &self.y else { unreachable!() };

        let member_state = y
            .members(group_id)
            .into_iter()
            .find(|(member_id, _state)| member_id == &member);

        let is_member = member_state.is_some();

        Ok(is_member)
    }

    /// Return `true` if the given member is currently assigned the `Pull` access level.
    fn is_puller(&self, group_id: ID, member: ID) -> Result<bool, Self::Error> {
        Ok(self.access(group_id, member)?.is_pull())
    }

    /// Return `true` if the given member is currently assigned the `Read` access level.
    fn is_reader(&self, group_id: ID, member: ID) -> Result<bool, Self::Error> {
        Ok(self.access(group_id, member)?.is_read())
    }

    /// Return `true` if the given member is currently assigned the `Write` access level.
    fn is_writer(&self, group_id: ID, member: ID) -> Result<bool, Self::Error> {
        Ok(self.access(group_id, member)?.is_write())
    }

    /// Return `true` if the given member is currently assigned the `Manage` access level.
    fn is_manager(&self, group_id: ID, member: ID) -> Result<bool, Self::Error> {
        Ok(self.access(group_id, member)?.is_manage())
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use rand::SeedableRng;
    use rand::rngs::StdRng;

    use crate::test_utils::partial_ord::{TestGroup, TestOrderer};
    use crate::test_utils::{Conditions, MemberId, MessageId, TestOperation, TestResolver};

    use super::*;

    const ALICE: char = 'A';
    const BOB: char = 'B';
    const CLAIRE: char = 'C';
    const DAVE: char = 'D';

    const MY_ID: char = 'T';
    const GROUP_ID: char = 'G';

    pub type TestGroups = Groups<MemberId, MessageId, Conditions, TestResolver, TestOrderer>;

    // Initialise the group manager and create a group with three initial members.
    fn setup() -> (TestGroups, TestOperation) {
        let initial_members = [
            (GroupMember::Individual(ALICE), Access::manage()),
            (GroupMember::Individual(BOB), Access::read()),
            (GroupMember::Individual(CLAIRE), Access::write()),
        ]
        .to_vec();

        let auth_heads_ref = Rc::new(RefCell::new(vec![]));
        let orderer_y = TestOrderer::init(MY_ID, auth_heads_ref.clone(), StdRng::from_os_rng());
        let y = TestGroup::init(orderer_y);
        let mut groups = TestGroups::new(MY_ID, y);
        let operation = groups.create(GROUP_ID, initial_members).unwrap();

        (groups, operation)
    }

    // The following tests are all focused on ensuring correct validation and returned error
    // variants. `GroupCrdt::prepare()` and `GroupCrdt::process()` are tested elsewhere and are therefore
    // excluded from explicit testing here.
    #[test]
    fn add_validation_errors() {
        let (mut groups, _) = setup();

        // Bob is not a manager.
        let _expected_access = <Access>::read();
        assert!(matches!(
            groups.add(GROUP_ID, BOB, DAVE, Access::pull()),
            Err(GroupsError::InsufficientAccess(
                BOB,
                _expected_access,
                GROUP_ID
            ))
        ));

        // Claire is already a group member.
        assert!(matches!(
            groups.add(GROUP_ID, ALICE, CLAIRE, Access::pull()),
            Err(GroupsError::GroupMember(CLAIRE, GROUP_ID))
        ));
    }

    #[test]
    fn remove_validation_errors() {
        let (mut groups, _) = setup();

        // Bob is not a manager.
        let _expected_access = <Access>::read();
        assert!(matches!(
            groups.remove(GROUP_ID, BOB, CLAIRE),
            Err(GroupsError::InsufficientAccess(
                BOB,
                _expected_access,
                GROUP_ID
            ))
        ));

        // Dave is not a group member.
        let err = groups.remove(GROUP_ID, ALICE, DAVE);
        assert!(
            matches!(err, Err(GroupsError::NotGroupMember(DAVE, GROUP_ID))),
            "{err:?}"
        );
    }

    #[test]
    fn promote_validation_errors() {
        let (mut groups, _) = setup();

        // Bob is not a manager.
        let _expected_access = <Access>::read();
        assert!(matches!(
            groups.promote(GROUP_ID, BOB, CLAIRE, Access::manage()),
            Err(GroupsError::InsufficientAccess(
                BOB,
                _expected_access,
                GROUP_ID
            ))
        ));

        // Dave is not a group member.
        assert!(matches!(
            groups.promote(GROUP_ID, ALICE, DAVE, Access::read()),
            Err(GroupsError::NotGroupMember(DAVE, GROUP_ID))
        ));

        // Bob already has `Read` access.
        let _expected_access = <Access>::read();
        assert!(matches!(
            groups.promote(GROUP_ID, ALICE, BOB, Access::read()),
            Err(GroupsError::SameAccessLevel(
                BOB,
                _expected_access,
                GROUP_ID
            ))
        ));
    }

    #[test]
    fn demote_validation_errors() {
        let (mut groups, _) = setup();

        // Bob is not a manager.
        let _expected_access = <Access>::read();
        assert!(matches!(
            groups.demote(GROUP_ID, BOB, CLAIRE, Access::pull()),
            Err(GroupsError::InsufficientAccess(
                BOB,
                _expected_access,
                GROUP_ID
            ))
        ));

        // Dave is not a group member.
        assert!(matches!(
            groups.demote(GROUP_ID, ALICE, DAVE, Access::read()),
            Err(GroupsError::NotGroupMember(DAVE, GROUP_ID))
        ));

        // Bob already has `Read` access.
        let _expected_access = <Access>::read();
        assert!(matches!(
            groups.demote(GROUP_ID, ALICE, BOB, Access::read()),
            Err(GroupsError::SameAccessLevel(
                BOB,
                _expected_access,
                GROUP_ID
            ))
        ));
    }
}
