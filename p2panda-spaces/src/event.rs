// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::{HashMap, HashSet};

use p2panda_core::VerifyingKey;
use serde::{Deserialize, Serialize};

use p2panda_auth::Access;
use p2panda_auth::group::GroupMember;
use p2panda_auth::traits::{Conditions, Operation};

use crate::auth::message::AuthMessage;
use crate::message::SpaceMembershipMessage;
use crate::types::{AuthGroupAction, AuthGroupState, EncryptionGroupOutput};
use crate::utils::{
    added_members, demoted_members, promoted_members, removed_members, sort_members,
};
use crate::{ActorId, GroupId, MemberId, SpaceId};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct GroupActor {
    id: ActorId,
    is_group: bool,
}

impl GroupActor {
    pub fn individual(id: MemberId) -> Self {
        Self {
            id,
            is_group: false,
        }
    }

    pub fn group(id: GroupId) -> Self {
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

/// Events emitted when system state changes or application messages are processed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[allow(clippy::large_enum_variant)]
pub enum Event<C> {
    Auth(GroupEvent<C>),
    Application { space_id: SpaceId, data: Vec<u8> },
    // @TODO: Could maybe add field to show when the bundle is valid until?
    KeyBundle { author: MemberId },
    Space(SpaceEvent<C>),
}

/// Additional context attached to group events.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupsContext<C> {
    /// The actor who authored this action.
    pub author: ActorId,

    /// Root group actors, can be individuals or groups.
    pub actors: Vec<(GroupActor, Access<C>)>,

    /// Members of this group.
    pub members: Vec<(ActorId, Access<C>)>,

    /// All groups for which this group is a child (direct or transitive).
    pub parents: Vec<ActorId>,

    /// All effected groups and their members.
    pub effected_members: HashMap<ActorId, Vec<(ActorId, Access<C>)>>,

    /// all effected groups and their actors.
    pub effected_actors: HashMap<ActorId, Vec<(GroupActor, Access<C>)>>,
}

/// Additional context attached to space events.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpaceContext<C> {
    /// The actor who applied this action to the spaces state.
    ///
    /// Note: this can be different to the author of the groups action in cases where concurrent
    /// auth changes which effect a space are applied later by other members.
    pub author: MemberId,

    /// Id of the group associated with this space.
    pub group_id: GroupId,

    /// Members in the spaces' space.
    pub members: Vec<(MemberId, Access<C>)>,

    /// Members in the spaces' space.
    pub actors: Vec<(GroupActor, Access<C>)>,
}

/// Events emitted when global auth state changes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum GroupEvent<C> {
    /// A group was created.
    Created {
        /// Group id.
        group_id: GroupId,

        /// Initial group members.
        initial_members: Vec<(GroupActor, Access<C>)>,

        /// Additional event context and group state after any change occurred.
        context: GroupsContext<C>,
    },

    /// A member was added to a group.
    Added {
        /// Group id.
        group_id: GroupId,

        /// Group actor that was added, can be individual or group.
        added: GroupActor,

        /// Access level assigned to the added members.
        access: Access<C>,

        /// Additional event context and group state after any change occurred.
        context: GroupsContext<C>,
    },

    /// A member was removed from a group.
    Removed {
        /// Group id.
        group_id: GroupId,

        /// Group actor that was removed, can be individual or group.
        removed: GroupActor,

        /// Additional event context and group state after any change occurred.
        context: GroupsContext<C>,
    },

    /// An existing group member was promoted.
    Promoted {
        /// Group id.
        group_id: GroupId,

        /// Group actor that was promoted, can be individual or group.
        promoted: GroupActor,

        /// Access level assigned to the promoted members.
        access: Access<C>,

        /// Additional event context and group state after any change occurred.
        context: GroupsContext<C>,
    },

    /// An existing group member was demoted.
    Demoted {
        /// Group id.
        group_id: GroupId,

        /// Group actor that was demoted, can be individual or group.
        demoted: GroupActor,

        /// Access level assigned to the demoted members.
        access: Access<C>,

        /// Additional event context and group state after any change occurred.
        context: GroupsContext<C>,
    },
}

impl<C> GroupEvent<C> {
    pub fn group_id(&self) -> GroupId {
        match self {
            GroupEvent::Created { group_id, .. } => *group_id,
            GroupEvent::Added { group_id, .. } => *group_id,
            GroupEvent::Removed { group_id, .. } => *group_id,
            GroupEvent::Promoted { group_id, .. } => *group_id,
            GroupEvent::Demoted { group_id, .. } => *group_id,
        }
    }

    pub fn context(&self) -> &GroupsContext<C> {
        match self {
            GroupEvent::Created { context, .. }
            | GroupEvent::Added { context, .. }
            | GroupEvent::Removed { context, .. }
            | GroupEvent::Promoted { context, .. }
            | GroupEvent::Demoted { context, .. } => context,
        }
    }

    pub fn effects(&self, group_id: GroupId) -> bool {
        let mut effected_parents = self.context().effected_actors.keys();

        if effected_parents.any(|id| *id == group_id) {
            return true;
        }

        false
    }
}

/// Events emitted when space encryption group membership changes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SpaceEvent<C> {
    /// A space was created.
    Created {
        /// Space id.
        space_id: SpaceId,

        /// Initial members in the space.
        initial_members: Vec<(MemberId, Access<C>)>,

        /// Additional event context and space state after any change occurred.
        context: SpaceContext<C>,

        /// Additional event context and group state after any change occurred.
        groups_context: GroupsContext<C>,
    },

    /// One or many individuals were added to the space.
    Added {
        /// Space id.
        space_id: SpaceId,

        /// Members added to the space.
        added: Vec<(MemberId, Access<C>)>,

        /// Additional event context and space state after any change occurred.
        context: SpaceContext<C>,

        /// Additional event context and group state after any change occurred.
        groups_context: GroupsContext<C>,
    },

    /// One or many individuals were removed from the space.
    Removed {
        /// Space id.
        space_id: SpaceId,

        /// Members removed from the space.
        removed: Vec<(MemberId, Access<C>)>,

        /// Additional event context and space state after any change occurred.
        context: SpaceContext<C>,

        /// Additional event context and group state after any change occurred.
        groups_context: GroupsContext<C>,
    },

    /// One or many individuals were promoted in the space.
    Promoted {
        /// Space id.
        space_id: SpaceId,

        /// Promoted members.
        promoted: Vec<(MemberId, Access<C>)>,

        /// Additional event context and space state after any change occurred.
        context: SpaceContext<C>,

        /// Additional event context and group state after any change occurred.
        groups_context: GroupsContext<C>,
    },

    /// One or many individuals were demoted in the space.
    Demoted {
        /// Space id.
        space_id: SpaceId,

        /// Demoted members.
        demoted: Vec<(MemberId, Access<C>)>,

        /// Additional event context and space state after any change occurred.
        context: SpaceContext<C>,

        /// Additional event context and group state after any change occurred.
        groups_context: GroupsContext<C>,
    },

    /// Local actor was removed from the space.
    Ejected {
        /// Space id.
        space_id: SpaceId,
    },
}

pub(crate) fn encryption_output_to_space_events<C>(
    space_id: &SpaceId,
    encryption_output: Vec<EncryptionGroupOutput>,
) -> Vec<Event<C>>
where
    C: Conditions,
{
    encryption_output
        .into_iter()
        .filter_map(|event| match event {
            EncryptionGroupOutput::Application { plaintext } => Some(Event::Application {
                space_id: *space_id,
                data: plaintext,
            }),
            _ => None,
        })
        .collect()
}

pub(crate) fn auth_message_to_group_event<C>(
    auth_y: &AuthGroupState<C>,
    auth_message: &AuthMessage<C>,
    previous_parents: &[MemberId],
) -> GroupEvent<C>
where
    C: Conditions,
{
    let group_id = auth_message.group_id();
    let context = groups_context(auth_y, auth_message, previous_parents);
    match auth_message.action() {
        AuthGroupAction::Create { .. } => GroupEvent::Created {
            group_id,
            initial_members: context.actors.clone(),
            context,
        },
        AuthGroupAction::Add { member, access } => GroupEvent::Added {
            group_id,
            added: GroupActor::from_group_member(member),
            access,
            context,
        },
        AuthGroupAction::Remove { member } => GroupEvent::Removed {
            group_id,
            removed: GroupActor::from_group_member(member),
            context,
        },
        AuthGroupAction::Promote { member, access } => GroupEvent::Promoted {
            group_id,
            promoted: GroupActor::from_group_member(member),
            access,
            context,
        },
        AuthGroupAction::Demote { member, access } => GroupEvent::Demoted {
            group_id,
            demoted: GroupActor::from_group_member(member),
            access,
            context,
        },
    }
}

pub(crate) fn group_message_to_auth_event<C>(
    auth_y: &AuthGroupState<C>,
    auth_message: &AuthMessage<C>,
    previous_parents: &[MemberId],
) -> Event<C>
where
    C: Conditions,
{
    let group_event = auth_message_to_group_event(auth_y, auth_message, previous_parents);
    Event::Auth(group_event)
}

pub(crate) fn space_message_to_space_event<C>(
    space_id: SpaceId,
    group_id: GroupId,
    auth_y: &AuthGroupState<C>,
    space_message: &SpaceMembershipMessage,
    auth_message: &AuthMessage<C>,
    previous_members: &[(MemberId, Access<C>)],
    previous_parents: &[MemberId],
) -> Event<C>
where
    C: Conditions,
{
    let next_members = &auth_y.members(group_id);
    let next_actors: Vec<_> = auth_y
        .root_members(group_id)
        .into_iter()
        .map(|(member, access)| (GroupActor::from_group_member(member), access))
        .collect();
    let context = SpaceContext {
        author: space_message.author,
        group_id,
        members: next_members.to_vec(),
        actors: next_actors,
    };
    let groups_context = groups_context(auth_y, auth_message, previous_parents);

    let space_event = match auth_message.action() {
        AuthGroupAction::Create { .. } => SpaceEvent::Created {
            space_id,
            initial_members: next_members.to_vec(),
            context,
            groups_context,
        },
        AuthGroupAction::Add { .. } => {
            let added = added_members(previous_members, next_members);
            SpaceEvent::Added {
                space_id,
                added,
                context,
                groups_context,
            }
        }
        AuthGroupAction::Remove { .. } => {
            let removed = removed_members(previous_members, next_members);
            SpaceEvent::Removed {
                space_id,
                removed,
                context,
                groups_context,
            }
        }
        AuthGroupAction::Promote { .. } => {
            let promoted = promoted_members(previous_members, next_members);
            SpaceEvent::Promoted {
                space_id,
                promoted,
                context,
                groups_context,
            }
        }
        AuthGroupAction::Demote { .. } => {
            let demoted = demoted_members(previous_members, next_members);
            SpaceEvent::Demoted {
                space_id,
                demoted,
                context,
                groups_context,
            }
        }
    };

    Event::Space(space_event)
}

fn groups_context<C>(
    auth_y: &AuthGroupState<C>,
    auth_message: &AuthMessage<C>,
    previous_parents: &[MemberId],
) -> GroupsContext<C>
where
    C: Conditions,
{
    let group_id = auth_message.group_id();

    let mut actors: Vec<_> = auth_y
        .root_members(group_id)
        .into_iter()
        .map(|(member, access)| (GroupActor::from_group_member(member), access))
        .collect();
    sort_members(&mut actors);

    let mut members = auth_y.members(group_id);
    sort_members(&mut members);

    let mut parents = auth_y.inner.parents(group_id);
    parents.sort();

    let effected: HashSet<&VerifyingKey> =
        HashSet::from_iter(parents.iter().chain(previous_parents.iter()));
    let effected_members: HashMap<ActorId, Vec<(ActorId, Access<C>)>> = effected
        .iter()
        .map(|id| (**id, auth_y.members(**id)))
        .collect();
    let effected_actors: HashMap<ActorId, Vec<(GroupActor, Access<C>)>> = effected
        .into_iter()
        .map(|id| {
            (
                *id,
                auth_y
                    .root_members(*id)
                    .into_iter()
                    .map(|(member, access)| (GroupActor::from_group_member(member), access))
                    .collect(),
            )
        })
        .collect();

    GroupsContext {
        author: auth_message.author(),
        members,
        actors,
        effected_members,
        effected_actors,
        parents,
    }
}
