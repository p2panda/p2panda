// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_auth::Access;
use p2panda_auth::group::GroupMember;
use p2panda_auth::traits::Conditions;
use p2panda_encryption::data_scheme::GroupOutput;

use crate::ActorId;
use crate::auth::message::AuthMessage;
use crate::message::{AuthoredMessage, SpacesArgs, SpacesMessage};
use crate::space::{added_members, removed_members};
use crate::traits::SpaceId;
use crate::types::{AuthGroupAction, AuthGroupState, EncryptionGroupOutput};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct GroupActor {
    id: ActorId,
    is_group: bool,
}

impl GroupActor {
    pub fn individual(id: ActorId) -> Self {
        Self {
            id,
            is_group: false,
        }
    }

    pub fn group(id: ActorId) -> Self {
        Self { id, is_group: true }
    }

    pub fn from_group_member(group_member: GroupMember<ActorId>) -> Self {
        match group_member {
            GroupMember::Individual(id) => GroupActor::individual(id),
            GroupMember::Group(id) => GroupActor::group(id),
        }
    }

    pub fn id(&self) -> ActorId {
        self.id
    }

    pub fn is_group(&self) -> bool {
        self.is_group
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(clippy::large_enum_variant)]
pub enum Event<ID, C> {
    Application { space_id: ID, data: Vec<u8> },
    Group(GroupEvent<C>),
    Space(SpaceEvent<ID>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GroupContext<C> {
    /// Root members, can be individuals or groups.
    pub group_actors: Vec<(GroupActor, Access<C>)>,

    /// Transitive members, can only be individuals.
    pub members: Vec<(ActorId, Access<C>)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpaceContext {
    /// Id of the group associated with this space.
    pub group_id: ActorId,

    /// Members in the spaces' encryption context.
    pub members: Vec<ActorId>,
}

/// Events emitted on auth group membership change.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GroupEvent<C> {
    /// A group was created.
    Created {
        /// Group id.
        group_id: ActorId,

        initial_members: Vec<(GroupActor, Access<C>)>,

        context: GroupContext<C>,
    },

    /// A member was added to a group.
    Added {
        /// Group id.
        group_id: ActorId,

        /// Member that was added, can be individual or group.
        added: GroupActor,

        /// Access level assigned to the added members.
        access: Access<C>,

        context: GroupContext<C>,
    },

    /// A member was removed from a group.
    Removed {
        /// Group id.
        group_id: ActorId,

        /// GroupActor that was removed, can be individual or group.
        removed: GroupActor,

        context: GroupContext<C>,
    },
}

/// Events emitted on space encryption group membership change.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SpaceEvent<ID> {
    /// A space was created.
    Created {
        /// Space id.
        space_id: ID,

        initial_members: Vec<ActorId>,

        context: SpaceContext,
    },

    /// One or many individuals were added to the space.
    Added {
        /// Space id.
        space_id: ID,

        /// Members added to the encryption context.
        added: Vec<ActorId>,

        context: SpaceContext,
    },

    /// One or many individuals were removed from the space.
    Removed {
        /// Space id.
        space_id: ID,

        /// Members removed from the encryption context.
        removed: Vec<ActorId>,

        context: SpaceContext,
    },

    /// Local actor was removed from the space.
    Ejected {
        /// Space id.
        space_id: ID,
    },
}

pub(crate) fn encryption_output_to_space_events<ID, M, C>(
    space_id: &ID,
    encryption_output: Vec<EncryptionGroupOutput<M>>,
) -> Vec<Event<ID, C>>
where
    ID: SpaceId,
    C: Conditions,
{
    encryption_output
        .into_iter()
        .filter_map(|event| match event {
            EncryptionGroupOutput::Application { plaintext } => Some(Event::Application {
                space_id: *space_id,
                data: plaintext,
            }),
            GroupOutput::Control(_control_message) => {
                // @TODO: when do control messages get emitted in group output?
                unimplemented!()
            }
            GroupOutput::Removed => Some(Event::Space(SpaceEvent::Ejected {
                space_id: *space_id,
            })),
        })
        .collect()
}

pub(crate) fn auth_message_to_group_event<ID, C>(
    auth_y: &AuthGroupState<C>,
    auth_message: &AuthMessage<C>,
) -> Event<ID, C>
where
    C: Conditions,
{
    let args = match auth_message {
        AuthMessage::Args(auth_args) => auth_args,
        AuthMessage::Forged { args, .. } => args,
    };

    let group_id = args.control_message.group_id();
    let group_actors = auth_y
        .root_members(group_id)
        .into_iter()
        .map(|(member, access)| (GroupActor::from_group_member(member), access))
        .collect();
    let members = auth_y.members(group_id);

    let group_event = match args.control_message.action.clone() {
        AuthGroupAction::Create { initial_members } => {
            let initial_members: Vec<(GroupActor, Access<C>)> = initial_members
                .into_iter()
                .map(|(member, access)| (GroupActor::from_group_member(member), access))
                .collect();
            let context = GroupContext {
                members,
                group_actors: initial_members.clone(),
            };
            GroupEvent::Created {
                group_id,
                initial_members,
                context,
            }
        }
        AuthGroupAction::Add { member, access } => GroupEvent::Added {
            group_id,
            added: GroupActor::from_group_member(member),
            access,
            context: GroupContext {
                group_actors,
                members,
            },
        },
        AuthGroupAction::Remove { member } => GroupEvent::Removed {
            group_id,
            removed: GroupActor::from_group_member(member),
            context: GroupContext {
                group_actors,
                members,
            },
        },
        AuthGroupAction::Promote { .. } => unimplemented!(),
        AuthGroupAction::Demote { .. } => unimplemented!(),
    };

    Event::Group(group_event)
}

pub(crate) fn space_message_to_space_event<ID, C, M>(
    space_message: &M,
    auth_message: &AuthMessage<C>,
    current_members: Vec<ActorId>,
    next_members: Vec<ActorId>,
) -> Event<ID, C>
where
    ID: SpaceId,
    C: Conditions,
    M: AuthoredMessage + SpacesMessage<ID, C>,
{
    let auth_args = match auth_message {
        AuthMessage::Args(auth_args) => auth_args,
        AuthMessage::Forged { args, .. } => args,
    };

    let SpacesArgs::SpaceMembership { space_id, .. } = space_message.args() else {
        panic!("unexpected message type");
    };
    let space_id = *space_id;

    let group_id = auth_args.control_message.group_id();
    let space_event = match auth_args.control_message.action.clone() {
        AuthGroupAction::Create { .. } => SpaceEvent::Created {
            space_id,
            initial_members: next_members.clone(),
            context: SpaceContext {
                group_id,
                members: next_members,
            },
        },
        AuthGroupAction::Add { .. } => {
            let added = added_members(current_members, next_members.clone());
            SpaceEvent::Added {
                space_id,
                added,
                context: SpaceContext {
                    group_id,
                    members: next_members,
                },
            }
        }
        AuthGroupAction::Remove { .. } => {
            let removed = removed_members(current_members, next_members.clone());
            SpaceEvent::Removed {
                space_id,
                removed,
                context: SpaceContext {
                    group_id,
                    members: next_members,
                },
            }
        }
        AuthGroupAction::Promote { .. } => unimplemented!(),
        AuthGroupAction::Demote { .. } => unimplemented!(),
    };

    Event::Space(space_event)
}
