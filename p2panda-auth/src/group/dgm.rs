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

impl<ID, OP, C, RS, ORD, GS> GroupMembership<ID, OP, C, GS, ORD>
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
    /// The caller of this method should ensure that the given `group_id` is unique. The set of
    /// initial members must not be empty; failure to meet these conditions will result in an error.
    /// Group updates will not be possible if the group is not created with at least one manager,
    /// since `Manage` access is required to perform any group state modifications.
    fn create(
        &self,
        group_id: ID,
        initial_members: Vec<(GroupMember<ID>, Access<C>)>,
    ) -> Result<(Self::State, ORD::Message), Self::Error> {
        if initial_members.is_empty() {
            return Err(GroupManagerError::EmptyGroup);
        }

        // TODO: Understand exactly what happens if two groups are created with the same
        // `group_id`. Does an error occur? If so, where and when?

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
        let y = GroupState::new(
            self.my_id,
            remote_operation.payload().group_id(),
            self.store.clone(),
            self.orderer.clone(),
        );

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
        if !Self::State::is_manager(&y, &adder) {
            let adder_access = Self::State::access(&y, &adder)?;
            return Err(GroupManagerError::InsufficientAccess(
                adder,
                adder_access,
                y.group_id,
            ));
        }

        if Self::State::is_member(&y, &added) {
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
    /// The `remover` must be a manager and the `removed` identity must already be a member of
    /// the group; failure to meet these conditions will result in an error.
    fn remove(
        y: Self::State,
        remover: ID,
        removed: ID,
    ) -> Result<(Self::State, ORD::Message), Self::Error> {
        if !Self::State::is_manager(&y, &remover) {
            let remover_access = Self::State::access(&y, &remover)?;
            return Err(GroupManagerError::InsufficientAccess(
                remover,
                remover_access,
                y.group_id,
            ));
        }

        if !Self::State::is_member(&y, &removed) {
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
    /// the group; failure to meet thess conditions will result in an error. A redundant access
    /// level assignment will also result in an error; for example, if the `promoted` member
    /// currently has `Read` access and the given access is also `Read`.
    fn promote(
        y: Self::State,
        promoter: ID,
        promoted: ID,
        access: Access<C>,
    ) -> Result<(Self::State, ORD::Message), Self::Error> {
        if !Self::State::is_manager(&y, &promoter) {
            let promoter_access = Self::State::access(&y, &promoter)?;
            return Err(GroupManagerError::InsufficientAccess(
                promoter,
                promoter_access,
                y.group_id,
            ));
        }

        if !Self::State::is_member(&y, &promoted) {
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
        if !Self::State::is_manager(&y, &demoter) {
            let demoter_access = Self::State::access(&y, &demoter)?;
            return Err(GroupManagerError::InsufficientAccess(
                demoter,
                demoter_access,
                y.group_id,
            ));
        }

        if !Self::State::is_member(&y, &demoted) {
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
