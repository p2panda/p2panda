// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_auth::Access;
use p2panda_auth::group::GroupMember;
use p2panda_auth::traits::Conditions;
use p2panda_encryption::data_scheme::{ControlMessage, GroupOutput};

use crate::ActorId;
use crate::auth::message::AuthMessage;
use crate::encryption::message::{EncryptionArgs, EncryptionMessage};
use crate::traits::SpaceId;
use crate::types::{AuthGroupAction, AuthGroupState, EncryptionGroupOutput};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Member {
    id: ActorId,
    is_group: bool,
}

impl Member {
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
            GroupMember::Individual(id) => Member::individual(id),
            GroupMember::Group(id) => Member::group(id),
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
pub enum Event<ID, C> {
    Application { space_id: ID, data: Vec<u8> },
    Group(GroupEvent<C>),
    Space(SpaceEvent<ID>),
}

/// Events emitted on auth group membership change.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GroupEvent<C> {
    /// A group was created.
    Created {
        /// Group id.
        group_id: ActorId,

        /// Root members, can be individuals or groups.
        members: Vec<(Member, Access<C>)>,

        /// Transitive members, can only be individuals.
        transitive_members: Vec<(Member, Access<C>)>,
    },
    /// A member was added to a group.
    Added {
        /// Group id.
        group_id: ActorId,

        /// Member that was added, can be individual or group.
        added: Member,

        /// Access level assigned to the added members.
        access: Access<C>,

        /// Root members, can be individuals or groups.
        members: Vec<(Member, Access<C>)>,

        /// Transitive members, can only be individuals.
        transitive_members: Vec<(Member, Access<C>)>,
    },
    /// A member was removed from a group.
    Removed {
        /// Group id.
        group_id: ActorId,

        /// Member that was removed, can be individual or group.
        removed: Member,

        /// Root members, can be individuals or groups.
        members: Vec<(Member, Access<C>)>,

        /// Transitive members, can only be individuals.
        transitive_members: Vec<(Member, Access<C>)>,
    },
}

/// Events emitted on space encryption group membership change.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SpaceEvent<ID> {
    /// A space was created.
    Created {
        /// Space id.
        space_id: ID,

        /// Id of the group associated with this space.
        group_id: ActorId,

        /// Individual in the spaces' encryption context.
        members: Vec<ActorId>,
    },
    /// An individual was added to the space.
    Added {
        /// Space id.
        space_id: ID,

        /// Id of the group associated with this space.
        group_id: ActorId,

        /// Individual added to the encryption context.
        added: ActorId,

        /// Individual in the spaces' encryption context.
        members: Vec<ActorId>,
    },
    /// An individual was removed from the space.
    Removed {
        /// Space id.
        space_id: ID,

        /// Id of the group associated with this space.
        group_id: ActorId,

        /// Individual removed from the encryption context.
        removed: ActorId,

        /// Individual in the spaces' encryption context.
        members: Vec<ActorId>,
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
                todo!()
            }
            GroupOutput::Removed => Some(Event::Space(SpaceEvent::Ejected {
                space_id: *space_id,
            })),
        })
        .collect()
}

pub(crate) fn encryption_message_to_space_event<ID, C>(
    space_id: &ID,
    group_id: ActorId,
    current_members: &mut Vec<ActorId>,
    encryption_message: &EncryptionMessage,
) -> Option<Event<ID, C>>
where
    ID: SpaceId,
    C: Conditions,
{
    let args = match encryption_message {
        EncryptionMessage::Args(auth_args) => auth_args,
        EncryptionMessage::Forged { args, .. } => args,
    };

    if let EncryptionArgs::System {
        control_message, ..
    } = args
    {
        let event = match control_message {
            ControlMessage::Create { initial_members } => SpaceEvent::Created {
                space_id: *space_id,
                group_id,
                members: initial_members.to_owned(),
            },
            ControlMessage::Remove { removed } => SpaceEvent::Removed {
                space_id: *space_id,
                group_id,
                removed: *removed,
                members: {
                    let (idx, _) = current_members
                        .iter()
                        .enumerate()
                        .find(|(_, member)| **member == *removed)
                        .expect("member exists");
                    current_members.remove(idx);
                    current_members.clone()
                },
            },
            ControlMessage::Add { added } => SpaceEvent::Added {
                space_id: *space_id,
                group_id,
                added: *added,
                members: {
                    current_members.push(*added);
                    current_members.clone()
                },
            },
            ControlMessage::Update => unimplemented!(),
        };
        return Some(Event::Space(event));
    }
    None
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
    let members = auth_y
        .root_members(group_id)
        .into_iter()
        .map(|(member, access)| (Member::from_group_member(member), access))
        .collect();
    let transitive_members = auth_y
        .members(group_id)
        .into_iter()
        .map(|(member, access)| (Member::individual(member), access))
        .collect();
    let group_event = match args.control_message.action.clone() {
        AuthGroupAction::Create { initial_members } => GroupEvent::Created {
            group_id,
            members: initial_members
                .into_iter()
                .map(|(member, access)| (Member::from_group_member(member), access))
                .collect(),
            transitive_members,
        },
        AuthGroupAction::Add { member, access } => GroupEvent::Added {
            group_id,
            added: Member::from_group_member(member),
            access,
            members,
            transitive_members,
        },
        AuthGroupAction::Remove { member } => GroupEvent::Removed {
            group_id,
            removed: Member::from_group_member(member),
            members,
            transitive_members,
        },
        AuthGroupAction::Promote { .. } => unimplemented!(),
        AuthGroupAction::Demote { .. } => unimplemented!(),
    };

    Event::Group(group_event)
}
