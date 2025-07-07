// SPDX-License-Identifier: MIT OR Apache-2.0

//! Group membership and authorisation.

mod action;
pub(crate) mod crdt;
#[cfg(any(test, feature = "test_utils"))]
mod display;
mod member;
mod message;
pub mod resolver;

pub use action::GroupAction;
pub use crdt::state::{GroupMembersState, GroupMembershipError, MemberState};
pub use crdt::{GroupCrdt, GroupCrdtError, GroupCrdtState, StateChangeResult};
pub use member::GroupMember;
pub use message::GroupControlMessage;

use std::fmt::{Debug, Display};
use std::marker::PhantomData;

use thiserror::Error;

use crate::Access;
use crate::traits::{
    Group as GroupTrait, GroupMembership, GroupStore, IdentityHandle, Operation, OperationId,
    Orderer, Resolver,
};

#[derive(Debug, Error)]
/// All possible errors that can occur when creating or updating a group.
pub enum GroupError<ID, OP, C, RS, ORD, GS>
where
    ID: IdentityHandle,
    OP: OperationId + Ord,
    RS: Resolver<ID, OP, C, ORD, GS>,
    ORD: Orderer<ID, OP, GroupControlMessage<ID, C>>,
    GS: GroupStore<ID, OP, C, RS, ORD>,
{
    #[error(transparent)]
    Group(#[from] GroupCrdtError<ID, OP, C, RS, ORD, GS>),

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
}

/// Decentralised Group Management (DGM).
///
/// The `Group` provides a high-level interface for creating and updating groups. These groups
/// provide a means for restricting access to application data and resources. Groups are
/// comprised of members, which may be individuals or groups, and are assigned a user-chosen
/// identity. Each member is assigned a unique user-chosen identifier and access level. Access
/// levels are used to enforce restrictions over access to data and the mutation of that data.
/// They are also used to grant permissions which allow for mutating the group state by adding,
/// removing and modifying the access level of other members.
///
/// Each `Group` method performs internal validation to ensure that the desired group action is
/// valid in light of the current group state. Attempting to perform an invalid action results in a
/// `GroupError`. For example, attempting to remove a member who is not currently part of the
/// group.
pub struct Group<ID, OP, C, RS, ORD, GS>
where
    ID: IdentityHandle,
    OP: OperationId + Ord,
    RS: Resolver<ID, OP, C, ORD, GS> + Debug,
    ORD: Orderer<ID, OP, GroupControlMessage<ID, C>>,
    GS: GroupStore<ID, OP, C, RS, ORD>,
{
    /// ID of the local actor.
    my_id: ID,

    /// Store for all known groups.
    store: GS,

    /// Message orderer state.
    orderer: ORD::State,

    _phantom: PhantomData<(ID, OP, C, RS, ORD, GS)>,
}

impl<ID, OP, C, RS, ORD, GS> Group<ID, OP, C, RS, ORD, GS>
where
    ID: IdentityHandle,
    OP: OperationId + Ord,
    RS: Resolver<ID, OP, C, ORD, GS> + Debug,
    ORD: Orderer<ID, OP, GroupControlMessage<ID, C>>,
    GS: GroupStore<ID, OP, C, RS, ORD>,
{
    /// Initialise the `Group` state so that groups can be created and updated.
    ///
    /// Requires the identifier of the local actor, as well as a group store and orderer.
    pub fn init(my_id: ID, store: GS, orderer: ORD::State) -> Self {
        Self {
            _phantom: PhantomData,
            my_id,
            store,
            orderer,
        }
    }
}

impl<ID, OP, C, RS, ORD, GS> GroupTrait<ID, OP, C, ORD> for Group<ID, OP, C, RS, ORD, GS>
where
    ID: IdentityHandle + Display,
    OP: OperationId + Ord + Display,
    C: Clone + Debug + PartialEq + PartialOrd,
    RS: Resolver<ID, OP, C, ORD, GS> + Debug,
    ORD: Orderer<ID, OP, GroupControlMessage<ID, C>> + Clone + Debug,
    ORD::Operation: Clone,
    ORD::State: Clone,
    GS: GroupStore<ID, OP, C, RS, ORD> + Clone + Debug,
{
    type State = GroupCrdtState<ID, OP, C, RS, ORD, GS>;
    type Action = GroupControlMessage<ID, C>;
    type Error = GroupError<ID, OP, C, RS, ORD, GS>;

    /// Create a group.
    ///
    /// The creator of the group is automatically added as a manager.
    ///
    /// The caller of this method must ensure that the given `group_id` is globally unique. For
    /// example, using a collision-resistant hash.
    fn create(
        &self,
        group_id: ID,
        members: Vec<(GroupMember<ID>, Access<C>)>,
    ) -> Result<(Self::State, ORD::Operation), Self::Error> {
        // The creator of the group is automatically added as a manager.
        let creator = (GroupMember::Individual(self.my_id), Access::manage());

        let mut initial_members = Vec::new();
        initial_members.push(creator);
        initial_members.extend(members);

        let y = GroupCrdtState::new(
            self.my_id,
            group_id,
            self.store.clone(),
            self.orderer.clone(),
        );

        let action = GroupControlMessage {
            group_id: y.group_id,
            action: GroupAction::Create { initial_members },
        };

        let (y, operation) = GroupCrdt::prepare(y, &action)?;
        let y = GroupCrdt::process(y, &operation)?;

        Ok((y, operation))
    }

    /// Create a group by processing a remote operation.
    fn create_from_remote(
        &self,
        remote_operation: ORD::Operation,
    ) -> Result<Self::State, Self::Error> {
        let group_id = remote_operation.payload().group_id();

        let y = GroupCrdtState::new(
            self.my_id,
            group_id,
            self.store.clone(),
            self.orderer.clone(),
        );

        let y = GroupCrdt::process(y, &remote_operation)?;

        Ok(y)
    }

    /// Update the group by processing a remotely-authored action.
    ///
    /// The `group_id` of the given operation must be the same as that of the given `y`; failure to
    /// meet this condition will result in an error.
    fn receive_from_remote(
        y: Self::State,
        remote_operation: ORD::Operation,
    ) -> Result<Self::State, Self::Error> {
        // Validation is performed internally by `process()`.
        let y = GroupCrdt::process(y, &remote_operation)?;

        Ok(y)
    }

    /// Add a group member.
    ///
    /// The `adder` must be a manager and the `added` identity must not already be a member of
    /// the group; failure to meet these conditions will result in an error.
    fn add(
        y: Self::State,
        adder: ID,
        added: ID,
        access: Access<C>,
    ) -> Result<(Self::State, ORD::Operation), Self::Error> {
        if !Self::State::is_manager(&y, &adder)? {
            let adder_access = Self::State::access(&y, &adder)?;
            return Err(GroupError::InsufficientAccess(
                adder,
                adder_access,
                y.group_id,
            ));
        }

        if Self::State::is_member(&y, &added)? {
            return Err(GroupError::GroupMember(added, y.group_id));
        }

        let action = GroupControlMessage {
            group_id: y.group_id,
            action: GroupAction::Add {
                member: GroupMember::Individual(added),
                access,
            },
        };

        let (y, operation) = GroupCrdt::prepare(y, &action)?;
        let y = GroupCrdt::process(y, &operation)?;

        Ok((y, operation))
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
        y: Self::State,
        remover: ID,
        removed: ID,
    ) -> Result<(Self::State, ORD::Operation), Self::Error> {
        if !Self::State::is_manager(&y, &remover)? {
            let remover_access = Self::State::access(&y, &remover)?;
            return Err(GroupError::InsufficientAccess(
                remover,
                remover_access,
                y.group_id,
            ));
        }

        if !Self::State::is_member(&y, &removed)? {
            return Err(GroupError::NotGroupMember(removed, y.group_id));
        }

        let action = GroupControlMessage {
            group_id: y.group_id,
            action: GroupAction::Remove {
                member: GroupMember::Individual(removed),
            },
        };

        let (y, operation) = GroupCrdt::prepare(y, &action)?;
        let y = GroupCrdt::process(y, &operation)?;

        Ok((y, operation))
    }

    /// Promote a group member to the given access level.
    ///
    /// The `promoter` must be a manager and the `promoted` identity must already be a member of
    /// the group; failure to meet these conditions will result in an error. A redundant access
    /// level assignment will also result in an error; for example, if the `promoted` member
    /// currently has `Read` access and the given access is also `Read`.
    fn promote(
        y: Self::State,
        promoter: ID,
        promoted: ID,
        access: Access<C>,
    ) -> Result<(Self::State, ORD::Operation), Self::Error> {
        if !Self::State::is_manager(&y, &promoter)? {
            let promoter_access = Self::State::access(&y, &promoter)?;
            return Err(GroupError::InsufficientAccess(
                promoter,
                promoter_access,
                y.group_id,
            ));
        }

        if !Self::State::is_member(&y, &promoted)? {
            return Err(GroupError::NotGroupMember(promoted, y.group_id));
        }

        // Prevent redundant access level assignment.
        if Self::State::access(&y, &promoted)? == access {
            return Err(GroupError::SameAccessLevel(promoted, access, y.group_id));
        }

        let action = GroupControlMessage {
            group_id: y.group_id,
            action: GroupAction::Promote {
                member: GroupMember::Individual(promoted),
                access,
            },
        };

        let (y, operation) = GroupCrdt::prepare(y, &action)?;
        let y = GroupCrdt::process(y, &operation)?;

        Ok((y, operation))
    }

    /// Demote a group member to the given access level.
    ///
    /// The `demoter` must be a manager and the `demoted` identity must already be a member of
    /// the group; failure to meet these conditions will result in an error. A redundant access
    /// level assignment will also result in an error; for example, if the `demoted` member
    /// currently has `Manage` access and the given access is also `Manage`.
    fn demote(
        y: Self::State,
        demoter: ID,
        demoted: ID,
        access: Access<C>,
    ) -> Result<(Self::State, ORD::Operation), Self::Error> {
        if !Self::State::is_manager(&y, &demoter)? {
            let demoter_access = Self::State::access(&y, &demoter)?;
            return Err(GroupError::InsufficientAccess(
                demoter,
                demoter_access,
                y.group_id,
            ));
        }

        if !Self::State::is_member(&y, &demoted)? {
            return Err(GroupError::NotGroupMember(demoted, y.group_id));
        }

        if Self::State::access(&y, &demoted)? == access {
            return Err(GroupError::SameAccessLevel(demoted, access, y.group_id));
        }

        let action = GroupControlMessage {
            group_id: y.group_id,
            action: GroupAction::Demote {
                member: GroupMember::Individual(demoted),
                access,
            },
        };

        let (y, operation) = GroupCrdt::prepare(y, &action)?;
        let y = GroupCrdt::process(y, &operation)?;

        Ok((y, operation))
    }
}

#[cfg(test)]
mod tests {
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    use crate::test_utils::{TestGroupState, TestGroupStore, TestOrdererState};

    use super::*;

    const ALICE: char = 'A';
    const BOB: char = 'B';
    const CLAIRE: char = 'C';
    const DAVE: char = 'D';

    const MY_ID: char = 'T';
    const GROUP_ID: char = 'G';

    // Initialise the group manager and create a group with three initial members.
    fn setup() -> TestGroupState {
        let rng = StdRng::from_os_rng();

        let store = TestGroupStore::default();
        let orderer = TestOrdererState::new(MY_ID, store.clone(), rng);

        let initial_members = [
            (GroupMember::Individual(ALICE), Access::manage()),
            (GroupMember::Individual(BOB), Access::read()),
            (GroupMember::Individual(CLAIRE), Access::write()),
        ]
        .to_vec();

        let group = Group::init(MY_ID, store, orderer);

        let (group_y, _operation) = group.create(GROUP_ID, initial_members).unwrap();

        group_y
    }

    // The following tests are all focused on ensuring correct validation and returned error
    // variants. `GroupCrdt::prepare()` and `GroupCrdt::process()` are tested elsewhere and are therefore
    // excluded from explicit testing here.
    #[test]
    fn add_validation_errors() {
        let y = setup();

        // Bob is not a manager.
        let _expected_access = <Access>::read();
        assert!(matches!(
            Group::add(y.clone(), BOB, DAVE, Access::pull()),
            Err(GroupError::InsufficientAccess(
                BOB,
                _expected_access,
                GROUP_ID
            ))
        ));

        // Claire is already a group member.
        assert!(matches!(
            Group::add(y, ALICE, CLAIRE, Access::pull()),
            Err(GroupError::GroupMember(CLAIRE, GROUP_ID))
        ));
    }

    #[test]
    fn remove_validation_errors() {
        let y = setup();

        // Bob is not a manager.
        let _expected_access = <Access>::read();
        assert!(matches!(
            Group::remove(y.clone(), BOB, CLAIRE),
            Err(GroupError::InsufficientAccess(
                BOB,
                _expected_access,
                GROUP_ID
            ))
        ));

        // Dave is not a group member.
        assert!(matches!(
            Group::remove(y, ALICE, DAVE),
            Err(GroupError::NotGroupMember(DAVE, GROUP_ID))
        ));
    }

    #[test]
    fn promote_validation_errors() {
        let y = setup();

        // Bob is not a manager.
        let _expected_access = <Access>::read();
        assert!(matches!(
            Group::promote(y.clone(), BOB, CLAIRE, Access::manage()),
            Err(GroupError::InsufficientAccess(
                BOB,
                _expected_access,
                GROUP_ID
            ))
        ));

        // Dave is not a group member.
        assert!(matches!(
            Group::promote(y.clone(), ALICE, DAVE, Access::read()),
            Err(GroupError::NotGroupMember(DAVE, GROUP_ID))
        ));

        // Bob already has `Read` access.
        let _expected_access = <Access>::read();
        assert!(matches!(
            Group::promote(y, ALICE, BOB, Access::read()),
            Err(GroupError::SameAccessLevel(BOB, _expected_access, GROUP_ID))
        ));
    }

    #[test]
    fn demote_validation_errors() {
        let y = setup();

        // Bob is not a manager.
        let _expected_access = <Access>::read();
        assert!(matches!(
            Group::demote(y.clone(), BOB, CLAIRE, Access::pull()),
            Err(GroupError::InsufficientAccess(
                BOB,
                _expected_access,
                GROUP_ID
            ))
        ));

        // Dave is not a group member.
        assert!(matches!(
            Group::demote(y.clone(), ALICE, DAVE, Access::read()),
            Err(GroupError::NotGroupMember(DAVE, GROUP_ID))
        ));

        // Bob already has `Read` access.
        let _expected_access = <Access>::read();
        assert!(matches!(
            Group::demote(y, ALICE, BOB, Access::read()),
            Err(GroupError::SameAccessLevel(BOB, _expected_access, GROUP_ID))
        ));
    }
}
