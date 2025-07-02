// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fmt::{Debug, Display};
use std::marker::PhantomData;

use thiserror::Error;

use crate::group::{
    Access, Group, GroupAction, GroupControlMessage, GroupError, GroupMember, GroupState,
};
use crate::traits::{
    AuthGroup, GroupMembership, GroupMembershipQuery, GroupStore, IdentityHandle, Operation,
    OperationId, Ordering, Resolver,
};

#[derive(Debug, Error)]
pub enum GroupManagerError<ID, OP, C, RS, ORD, GS>
where
    ID: IdentityHandle,
    OP: OperationId + Ord,
    RS: Resolver<ORD::Message>,
    ORD: Ordering<ID, OP, GroupControlMessage<ID, OP, C>>,
    GS: GroupStore<ID, OP, C, RS, ORD>,
{
    #[error(transparent)]
    Group(#[from] GroupError<ID, OP, C, RS, ORD, GS>),

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
/// The `GroupManager` provides a high-level interface for creating and updating groups. These
/// groups provide a means for restricting access to application data and resources. Groups are
/// comprised of members, which may be individuals or groups, and are assigned a user-chosen
/// identity. Each member is assigned a unique user-chosen identifier and access level. Access
/// levels are used to enforce restrictions over access to data and the mutation of that data.
/// They are also used to grant permissions which allow for mutating the group state by adding,
/// removing and modifying the access level of other members.
///
/// Each `GroupManager` method performs internal validation to ensure that the desired group
/// action is valid in light of the current group state. Attempting to perform an invalid action
/// results in a `GroupManagerError`. For example, attempting to remove a member who is not
/// currently part of the group.
pub struct GroupManager<ID, OP, C, RS, ORD, GS>
where
    ID: IdentityHandle,
    OP: OperationId + Ord,
    RS: Resolver<ORD::Message>,
    ORD: Ordering<ID, OP, GroupControlMessage<ID, OP, C>>,
    GS: GroupStore<ID, OP, C, RS, ORD>,
{
    /// ID of the local actor.
    my_id: ID,

    /// Store for all locally-known groups.
    store: GS,

    /// Message orderer state.
    orderer: ORD::State,

    _phantom: PhantomData<(ID, OP, C, RS, ORD, GS)>,
}

impl<ID, OP, C, RS, ORD, GS> GroupManager<ID, OP, C, RS, ORD, GS>
where
    ID: IdentityHandle,
    OP: OperationId + Ord,
    RS: Resolver<ORD::Message>,
    ORD: Ordering<ID, OP, GroupControlMessage<ID, OP, C>>,
    GS: GroupStore<ID, OP, C, RS, ORD>,
{
    pub fn init(my_id: ID, store: GS, orderer: ORD::State) -> Self {
        Self {
            _phantom: PhantomData,
            my_id,
            store,
            orderer,
        }
    }
}

impl<ID, OP, C, RS, ORD, GS> GroupMembership<ID, OP, C, ORD>
    for GroupManager<ID, OP, C, RS, ORD, GS>
where
    ID: IdentityHandle + Display,
    OP: OperationId + Ord + Display,
    C: Clone + Debug + PartialEq + PartialOrd,
    RS: Resolver<ORD::Message, State = GroupState<ID, OP, C, RS, ORD, GS>> + Debug,
    ORD: Ordering<ID, OP, GroupControlMessage<ID, OP, C>> + Clone + Debug,
    ORD::Message: Clone,
    ORD::State: Clone,
    GS: GroupStore<ID, OP, C, RS, ORD> + Clone + Debug,
{
    type State = GroupState<ID, OP, C, RS, ORD, GS>;
    type Action = GroupControlMessage<ID, OP, C>;
    type Error = GroupManagerError<ID, OP, C, RS, ORD, GS>;

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
    ) -> Result<(Self::State, ORD::Message), Self::Error> {
        // The creator of the group is automatically added as a manager.
        let creator = (GroupMember::Individual(self.my_id), Access::Manage);

        let mut initial_members = Vec::new();
        initial_members.push(creator);
        initial_members.extend(members);

        let y = GroupState::new(
            self.my_id,
            group_id,
            self.store.clone(),
            self.orderer.clone(),
        );

        let action = GroupControlMessage::GroupAction {
            group_id: y.group_id,
            action: GroupAction::Create { initial_members },
        };

        let (y, operation) = Group::prepare(y, &action)?;
        let y = Group::process(y, &operation)?;

        Ok((y, operation))
    }

    /// Create a group by processing a remote operation.
    fn create_from_remote(
        &self,
        remote_operation: ORD::Message,
    ) -> Result<Self::State, Self::Error> {
        let group_id = remote_operation.payload().group_id();

        let y = GroupState::new(
            self.my_id,
            group_id,
            self.store.clone(),
            self.orderer.clone(),
        );

        let y = Group::process(y, &remote_operation)?;

        Ok(y)
    }

    /// Update the group by processing a remotely-authored action.
    ///
    /// The `group_id` of the given operation must be the same as that of the given `y`; failure to
    /// meet this condition will result in an error.
    fn receive_from_remote(
        y: Self::State,
        remote_operation: ORD::Message,
    ) -> Result<Self::State, Self::Error> {
        // Validation is performed internally by `process()`.
        let y = Group::process(y, &remote_operation)?;

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
    ) -> Result<(Self::State, ORD::Message), Self::Error> {
        if !Self::State::is_manager(&y, &adder)? {
            let adder_access = Self::State::access(&y, &adder)?;
            return Err(GroupManagerError::InsufficientAccess(
                adder,
                adder_access,
                y.group_id,
            ));
        }

        if Self::State::is_member(&y, &added)? {
            return Err(GroupManagerError::GroupMember(added, y.group_id));
        }

        let action = GroupControlMessage::GroupAction {
            group_id: y.group_id,
            action: GroupAction::Add {
                member: GroupMember::Individual(added),
                access,
            },
        };

        let (y, operation) = Group::prepare(y, &action)?;
        let y = Group::process(y, &operation)?;

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
    ) -> Result<(Self::State, ORD::Message), Self::Error> {
        if !Self::State::is_manager(&y, &remover)? {
            let remover_access = Self::State::access(&y, &remover)?;
            return Err(GroupManagerError::InsufficientAccess(
                remover,
                remover_access,
                y.group_id,
            ));
        }

        if !Self::State::is_member(&y, &removed)? {
            return Err(GroupManagerError::NotGroupMember(removed, y.group_id));
        }

        let action = GroupControlMessage::GroupAction {
            group_id: y.group_id,
            action: GroupAction::Remove {
                member: GroupMember::Individual(removed),
            },
        };

        let (y, operation) = Group::prepare(y, &action)?;
        let y = Group::process(y, &operation)?;

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
    ) -> Result<(Self::State, ORD::Message), Self::Error> {
        if !Self::State::is_manager(&y, &promoter)? {
            let promoter_access = Self::State::access(&y, &promoter)?;
            return Err(GroupManagerError::InsufficientAccess(
                promoter,
                promoter_access,
                y.group_id,
            ));
        }

        if !Self::State::is_member(&y, &promoted)? {
            return Err(GroupManagerError::NotGroupMember(promoted, y.group_id));
        }

        // Prevent redundant access level assignment.
        if Self::State::access(&y, &promoted)? == access {
            return Err(GroupManagerError::SameAccessLevel(
                promoted, access, y.group_id,
            ));
        }

        let action = GroupControlMessage::GroupAction {
            group_id: y.group_id,
            action: GroupAction::Promote {
                member: GroupMember::Individual(promoted),
                access,
            },
        };

        let (y, operation) = Group::prepare(y, &action)?;
        let y = Group::process(y, &operation)?;

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
    ) -> Result<(Self::State, ORD::Message), Self::Error> {
        if !Self::State::is_manager(&y, &demoter)? {
            let demoter_access = Self::State::access(&y, &demoter)?;
            return Err(GroupManagerError::InsufficientAccess(
                demoter,
                demoter_access,
                y.group_id,
            ));
        }

        if !Self::State::is_member(&y, &demoted)? {
            return Err(GroupManagerError::NotGroupMember(demoted, y.group_id));
        }

        if Self::State::access(&y, &demoted)? == access {
            return Err(GroupManagerError::SameAccessLevel(
                demoted, access, y.group_id,
            ));
        }

        let action = GroupControlMessage::GroupAction {
            group_id: y.group_id,
            action: GroupAction::Demote {
                member: GroupMember::Individual(demoted),
                access,
            },
        };

        let (y, operation) = Group::prepare(y, &action)?;
        let y = Group::process(y, &operation)?;

        Ok((y, operation))
    }
}

#[cfg(test)]
mod tests {
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    use crate::group::test_utils::{TestGroupState, TestGroupStore, TestOrdererState};

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
            (GroupMember::Individual(ALICE), Access::Manage),
            (GroupMember::Individual(BOB), Access::Read),
            (
                GroupMember::Individual(CLAIRE),
                Access::Write { conditions: None },
            ),
        ]
        .to_vec();

        let group = GroupManager::init(MY_ID, store, orderer);

        let (group_y, _operation) = group.create(GROUP_ID, initial_members).unwrap();

        group_y
    }

    // The following tests are all focused on ensuring correct validation and returned error
    // variants. `Group::prepare()` and `Group::process()` are tested elsewhere and are therefore
    // excluded from explicit testing here.
    #[test]
    fn add_validation_errors() {
        let y = setup();

        // Bob is not a manager.
        assert!(matches!(
            GroupManager::add(y.clone(), BOB, DAVE, Access::Pull),
            Err(GroupManagerError::InsufficientAccess(
                BOB,
                Access::Read,
                GROUP_ID
            ))
        ));

        // Claire is already a group member.
        assert!(matches!(
            GroupManager::add(y, ALICE, CLAIRE, Access::Pull),
            Err(GroupManagerError::GroupMember(CLAIRE, GROUP_ID))
        ));
    }

    #[test]
    fn remove_validation_errors() {
        let y = setup();

        // Bob is not a manager.
        assert!(matches!(
            GroupManager::remove(y.clone(), BOB, CLAIRE),
            Err(GroupManagerError::InsufficientAccess(
                BOB,
                Access::Read,
                GROUP_ID
            ))
        ));

        // Dave is not a group member.
        assert!(matches!(
            GroupManager::remove(y, ALICE, DAVE),
            Err(GroupManagerError::NotGroupMember(DAVE, GROUP_ID))
        ));
    }

    #[test]
    fn promote_validation_errors() {
        let y = setup();

        // Bob is not a manager.
        assert!(matches!(
            GroupManager::promote(y.clone(), BOB, CLAIRE, Access::Manage),
            Err(GroupManagerError::InsufficientAccess(
                BOB,
                Access::Read,
                GROUP_ID
            ))
        ));

        // Dave is not a group member.
        assert!(matches!(
            GroupManager::promote(y.clone(), ALICE, DAVE, Access::Read),
            Err(GroupManagerError::NotGroupMember(DAVE, GROUP_ID))
        ));

        // Bob already has `Read` access.
        assert!(matches!(
            GroupManager::promote(y, ALICE, BOB, Access::Read),
            Err(GroupManagerError::SameAccessLevel(
                BOB,
                Access::Read,
                GROUP_ID
            ))
        ));
    }

    #[test]
    fn demote_validation_errors() {
        let y = setup();

        // Bob is not a manager.
        assert!(matches!(
            GroupManager::demote(y.clone(), BOB, CLAIRE, Access::Pull),
            Err(GroupManagerError::InsufficientAccess(
                BOB,
                Access::Read,
                GROUP_ID
            ))
        ));

        // Dave is not a group member.
        assert!(matches!(
            GroupManager::demote(y.clone(), ALICE, DAVE, Access::Read),
            Err(GroupManagerError::NotGroupMember(DAVE, GROUP_ID))
        ));

        // Bob already has `Read` access.
        assert!(matches!(
            GroupManager::demote(y, ALICE, BOB, Access::Read),
            Err(GroupManagerError::SameAccessLevel(
                BOB,
                Access::Read,
                GROUP_ID
            ))
        ));
    }
}
