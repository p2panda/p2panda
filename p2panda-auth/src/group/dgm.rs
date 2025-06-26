// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fmt::{Debug, Display};
use std::marker::PhantomData;

use thiserror::Error;

use crate::group::{
    Access, Group, GroupAction, GroupControlMessage, GroupError, GroupMember, GroupState,
};
use crate::traits::{
    AuthGroup, GroupMembership, GroupMembershipQuery, GroupStore, IdentityHandle, OperationId,
    Ordering, Resolver,
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

    #[error("actor {0} is already a member of group {1}")]
    GroupMember(ID, ID),

    #[error("actor {0} is not a member of group {1}")]
    NotGroupMember(ID, ID),

    #[error("action requires manager access but actor {0} is {1} in group {2}")]
    InsufficientAccess(ID, Access<C>, ID),

    #[error("actor {0} already has access level {1} in group {2}")]
    SameAccessLevel(ID, Access<C>, ID),
}

pub struct GroupManager<ID, OP, C, RS, ORD, GS>
where
    ID: IdentityHandle,
    OP: OperationId + Ord,
    RS: Resolver<ORD::Message>,
    ORD: Ordering<ID, OP, GroupControlMessage<ID, OP, C>>,
    GS: GroupStore<ID, OP, C, RS, ORD>,
{
    // TODO: Do we want to store state here or go purely functional?
    // If not here, where? Then we're probably passing the responsibility to the user to hold it
    // somewhere appropriate. This will likely become clearer during integration work.
    _phantom: PhantomData<(ID, OP, C, RS, ORD, GS)>,
}

impl<ID, OP, C, RS, ORD, GS> GroupMembership<ID, OP, C, GS, ORD>
    for GroupManager<ID, OP, C, RS, ORD, GS>
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
    type Error = GroupManagerError<ID, OP, C, RS, ORD, GS>;

    fn init(
        my_id: ID,
        group_id: ID,
        store: GS,
        orderer: ORD::State,
    ) -> Result<Self::State, Self::Error> {
        let y = GroupState::new(my_id, group_id, store, orderer);

        Ok(y)
    }

    fn create(
        y: Self::State,
        initial_members: Vec<(GroupMember<ID>, Access<C>)>,
    ) -> Result<(Self::State, ORD::Message), Self::Error> {
        let action = GroupControlMessage::GroupAction {
            group_id: y.group_id,
            action: GroupAction::Create { initial_members },
        };

        let (y, operation) = Group::prepare(y, &action)?;
        let y = Group::process(y, &operation)?;

        Ok((y, operation))
    }

    fn create_from_remote(
        y: Self::State,
        remote_operation: ORD::Message,
    ) -> Result<Self::State, Self::Error> {
        let y = Group::process(y, &remote_operation)?;

        Ok(y)
    }

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
