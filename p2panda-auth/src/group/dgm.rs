// SPDX-License-Identifier: MIT OR Apache-2.0

// TODO: Rename this to `src/group/dgm.rs`.

use std::fmt::{Debug, Display};

use thiserror::Error;

use crate::group::{
    Access, Group, GroupAction, GroupControlMessage, GroupError, GroupMember, GroupState,
};
use crate::traits::{
    AuthGroup, GroupMembership, GroupMembershipQuery, GroupStore, IdentityHandle, OperationId,
    Ordering, Resolver,
};

use serde::{Deserialize, Serialize};

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

    // TODO: We already have a `GroupMembershipError` which covers this case.
    // Be sure we're not creating duplicate errors.
    // Either allow lower-level errors to bubble up or unify the type into this variant.
    #[error("action requires manager access but actor is {0}")]
    InsufficientAuthority(Access<C>),
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
    // somewhere appropriate.
    _state: GroupState<ID, OP, C, RS, ORD, GS>,
}

// TODO: More validation?

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

    // TODO: Pass in the store and orderer here (for now...review during integration process).
    // See L175 in `src/group.mod.rs`
    fn create(
        my_id: ID,
        group_id: ID,
        initial_members: Vec<(GroupMember<ID>, Access<C>)>,
        store: GS,
        orderer: ORD::State,
    ) -> Result<(Self::State, ORD::Message), Self::Error> {
        let y = GroupState::new(my_id, group_id, store, orderer);

        let action = GroupControlMessage::GroupAction {
            group_id: y.group_id,
            action: GroupAction::Create { initial_members },
        };

        let (y, operation) = Group::prepare(y, &action)?;
        let y = Group::process(y, &operation)?;

        Ok((y, operation))
    }

    fn add(
        y: Self::State,
        adder: ID,
        added: ID,
        access: Access<C>,
    ) -> Result<(Self::State, ORD::Message), Self::Error> {
        if !Self::State::is_manager(&y, &adder) {
            let adder_access = Self::State::access(&y, &adder)?;
            return Err(GroupManagerError::InsufficientAuthority(adder_access));
        }

        let action = GroupControlMessage::GroupAction {
            group_id: y.group_id,
            action: GroupAction::Add {
                member: GroupMember::Individual(added),
                access,
            },
        };

        // TODO: Possibly another validation check. Is `added` already part of the group?
        let (y, operation) = Group::prepare(y, &action)?;
        // At this point you've already trusted that the operation should be included in the group.
        // The operation will still be appended to the graph, even if it ends up being invalid.
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
            return Err(GroupManagerError::InsufficientAuthority(remover_access));
        }

        let action = GroupControlMessage::GroupAction {
            group_id: y.group_id,
            action: GroupAction::Remove {
                member: GroupMember::Individual(removed),
            },
        };

        // TODO: Possibly another validation check. Is `removed` a current member of the group?
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
            return Err(GroupManagerError::InsufficientAuthority(promoter_access));
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
            return Err(GroupManagerError::InsufficientAuthority(demoter_access));
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
