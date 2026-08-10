// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::{HashMap, HashSet};

use p2panda_auth::Access;
use p2panda_auth::group::GroupMember;
use p2panda_auth::traits::{Conditions, Operation};
use p2panda_core::VerifyingKey;

use crate::auth::message::AuthMessage;
use crate::member::Member;
use crate::message::SpaceMembershipMessage;
use crate::space::SpacesState;
use crate::types::{AuthGroupAction, AuthGroupState, EncryptionGroupOutput};
use crate::utils::{
    added_members, demoted_members, promoted_members, removed_members, sort_members,
};
use crate::{ActorId, GroupId, MemberId, OperationId, SpaceId};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
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
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(clippy::large_enum_variant)]
pub enum Event<C> {
    /// A member's info with associated key-bundle has been received.
    Member(Member),

    /// A group membership change occurred in the shared groups state.
    ///
    /// This event does _not_ signify that any space has incorporated this change yet. The
    /// Event::Spaces variant is emitted on space membership changes.
    Groups(GroupEvent<C>),

    /// An application message was decrypted.
    ///
    /// Encrypted application messages are buffered until the local member is welcomed into a
    /// space with a "create" or "add" message.
    Application { space_id: SpaceId, data: Vec<u8> },

    /// A membership change occurred on a space.
    ///
    /// This event is emitted every time the membership of a space changes. Events are silently
    /// dropped if the local member is not (yet) a member of the space. For every Event::Groups
    /// event a Event::Spaces event will be emitted for every effected space.
    Spaces(SpaceEvent<C>),
}

/// Additional context attached to group events.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GroupContext<C> {
    /// The actor who authored the associated group action.
    pub author: ActorId,

    /// Root group actors, can be individuals or groups.
    pub actors: Vec<(GroupActor, Access<C>)>,

    /// Members of the group.
    pub members: Vec<(ActorId, Access<C>)>,

    /// All groups for which the group is a child (direct or transitive).
    pub ancestors: Vec<ActorId>,

    /// All groups effected by the associated group change and their members.
    pub effected_group_members: HashMap<ActorId, Vec<(ActorId, Access<C>)>>,

    /// All groups effected by the associated group change and their direct actor members.
    pub effected_group_actors: HashMap<ActorId, Vec<(GroupActor, Access<C>)>>,
}

/// Additional context attached to space events.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpaceContext<C> {
    /// The actor who applied this action to the spaces state.
    ///
    /// Note: this can be different to the author of the groups action in cases where concurrent
    /// auth changes which effect a space are applied later by other members.
    pub author: MemberId,

    /// Id of the group associated with the space.
    pub group_id: GroupId,

    /// Current members of the space.
    pub members: Vec<(MemberId, Access<C>)>,

    /// Current direct actor members of the space.
    pub actors: Vec<(GroupActor, Access<C>)>,
}

/// Events emitted when global auth state changes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GroupEvent<C> {
    /// A group was created.
    Created {
        /// Group id.
        group_id: GroupId,

        /// Initial group members.
        initial_members: Vec<(GroupActor, Access<C>)>,

        /// Additional event context and group state after any change occurred.
        context: GroupContext<C>,
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
        context: GroupContext<C>,
    },

    /// A member was removed from a group.
    Removed {
        /// Group id.
        group_id: GroupId,

        /// Group actor that was removed, can be individual or group.
        removed: GroupActor,

        /// Additional event context and group state after any change occurred.
        context: GroupContext<C>,
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
        context: GroupContext<C>,
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
        context: GroupContext<C>,
    },
}

impl<C> GroupEvent<C> {
    /// The target group of this event.
    pub fn group_id(&self) -> GroupId {
        match self {
            GroupEvent::Created { group_id, .. } => *group_id,
            GroupEvent::Added { group_id, .. } => *group_id,
            GroupEvent::Removed { group_id, .. } => *group_id,
            GroupEvent::Promoted { group_id, .. } => *group_id,
            GroupEvent::Demoted { group_id, .. } => *group_id,
        }
    }

    /// The groups context attached to this event.
    pub fn context(&self) -> &GroupContext<C> {
        match self {
            GroupEvent::Created { context, .. }
            | GroupEvent::Added { context, .. }
            | GroupEvent::Removed { context, .. }
            | GroupEvent::Promoted { context, .. }
            | GroupEvent::Demoted { context, .. } => context,
        }
    }

    /// Returns true if the passed group was effected by the action which triggered this event.
    ///
    /// An effected group is one whose membership changes as a result of the events' action,
    /// including ancestor groups who transitively contain the actions target group as member. A
    /// change could be if a member was added, removed, promoted or demoted.
    ///
    ///
    /// ```text
    ///    [group A]   [group B]   [group C]
    ///           |     |
    ///           v     v
    ///          [group D]
    /// ```
    ///
    /// In the above example, group D is effected by events for group A & B but not C.
    pub fn effected_group(&self, group_id: GroupId) -> bool {
        let mut effected_group = self.context().effected_group_actors.keys();

        if effected_group.any(|id| *id == group_id) {
            return true;
        }

        false
    }
}

/// Events emitted when space membership changes.
#[derive(Clone, Debug, PartialEq, Eq)]
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
        groups_context: GroupContext<C>,
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
        groups_context: GroupContext<C>,
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
        groups_context: GroupContext<C>,

        /// Effected concurrent application messages.
        effected_application_messages: Vec<OperationId>,
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
        groups_context: GroupContext<C>,
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
        groups_context: GroupContext<C>,

        /// Effected concurrent application messages.
        ///
        /// Will only ever be populated if member was demoted to below "write" access.
        effected_application_messages: Vec<OperationId>,
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

pub(crate) fn to_groups_event<C>(
    auth_y: &AuthGroupState<C>,
    auth_message: &AuthMessage<C>,
    previous_ancestors: &[MemberId],
) -> Event<C>
where
    C: Conditions,
{
    let group_id = auth_message.group_id();
    let context = groups_context(auth_y, auth_message, previous_ancestors);
    let group_event = match auth_message.action() {
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
    };
    Event::Groups(group_event)
}

pub(crate) fn to_space_event<C>(
    y: &SpacesState<C>,
    space_message: &SpaceMembershipMessage,
    auth_message: &AuthMessage<C>,
    previous_members: &[(MemberId, Access<C>)],
    previous_ancestors: &[MemberId],
) -> Event<C>
where
    C: Conditions,
{
    let space_id = y.space_id;
    let group_id = y.group_id;
    let next_members = &y.groups_y.members(group_id);
    let next_actors: Vec<_> = y
        .groups_y
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
    let groups_context = groups_context(&y.groups_y, auth_message, previous_ancestors);

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
            // If this is a remove message we need to collect any concurrent application messages
            // which may be effected by the change.
            let removed_authors: Vec<VerifyingKey> =
                removed.iter().map(|(member, _)| *member).collect();
            let effected_application_messages = detect_concurrent_app_messages(
                y,
                space_message.id,
                auth_message.id(),
                &removed_authors,
            );

            SpaceEvent::Removed {
                space_id,
                removed,
                context,
                groups_context,
                effected_application_messages,
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
            let non_write_demoted_members: Vec<_> = demoted
                .iter()
                .filter_map(|(member, access)| {
                    if access < &Access::<C>::write() {
                        Some(*member)
                    } else {
                        None
                    }
                })
                .collect();
            let effected_application_messages = detect_concurrent_app_messages(
                y,
                space_message.id,
                auth_message.id(),
                &non_write_demoted_members,
            );

            SpaceEvent::Demoted {
                space_id,
                demoted,
                context,
                groups_context,
                effected_application_messages,
            }
        }
    };

    Event::Spaces(space_event)
}

/// Detect any application messages which were authored by a concurrently removed author.
///
/// There are two possible concurrency cases to detect, one can be inferred by inspecting the
/// "proof" (a concrete point in the groups operation graph) an application message carries, the
/// other by observing concurrency in the space operation graph itself.
///
/// Case 1: proof is concurrent
///
/// The application message is referring to a proof which is entirely concurrent to the target
/// remove operation.
///
/// ```text
///    Remove A                              ...          
///       │                                   │           
///       │                                   ▼           
///       │   Add B ◄─────────────────────── App          
///       │    │            proof             │           
///       │    ▼                              ▼           
///       └─► Add A                          ...          
///            │                              │           
///            ▼                              ▼           
///          Group                          Space         
///                                                                                                          
/// ```  
///
/// Case 2: space operation is concurrent                                                    
///                                     
/// The application message is published concurrently to the membership message which incorporated
/// the target remove into the space.
///
/// ```text
///     Remove A ◄─────────────────────── Membership         
///       │                                   │              
///       │                                   ▼              
///       │   Add B ◄──────────────────── Membership      App
///       │    │                              │            │
///       │    ▼                              ▼            │
///       └─► Add A ◄───────────────────  Membership ◄─────┘
///            │                              │              
///            ▼                              ▼              
///          Group                          Space      
/// ```      
fn detect_concurrent_app_messages<C: Conditions>(
    y: &SpacesState<C>,
    space_message_id: OperationId,
    group_message_id: OperationId,
    removed_authors: &[VerifyingKey],
) -> Vec<OperationId> {
    let concurrent_application_messages = y
        .encryption_y
        .orderer
        .concurrent_application_messages(space_message_id);
    let concurrent_groups_messages = y.groups_y.inner.concurrent_operations(group_message_id);
    let mut effected_application_messages = vec![];
    for (id, (author, proofs)) in y.proofs.iter() {
        if !removed_authors.contains(author) {
            // This operation author is not effected.
            continue;
        };

        // If an application messages refers to _only_ concurrent branches in it's proof
        // OR the application message was published concurrently to the space message
        // which incorporated this group change then add it to the effected messages vec.
        if concurrent_groups_messages.is_superset(proofs)
            || concurrent_application_messages.contains(id)
        {
            effected_application_messages.push(*id);
        }
    }
    effected_application_messages
}

/// Compute groups context.
fn groups_context<C>(
    auth_y: &AuthGroupState<C>,
    auth_message: &AuthMessage<C>,
    previous_ancestors: &[MemberId],
) -> GroupContext<C>
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

    let mut ancestors = auth_y.inner.ancestors(group_id);
    ancestors.sort();

    // Retrieve members of all effected groups.
    let effected: HashSet<&VerifyingKey> =
        HashSet::from_iter(ancestors.iter().chain(previous_ancestors.iter()));
    let effected_group_members: HashMap<ActorId, Vec<(ActorId, Access<C>)>> = effected
        .iter()
        .map(|id| (**id, auth_y.members(**id)))
        .collect();
    let effected_group_actors: HashMap<ActorId, Vec<(GroupActor, Access<C>)>> = effected
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

    GroupContext {
        author: auth_message.author(),
        members,
        actors,
        effected_group_members,
        effected_group_actors,
        ancestors,
    }
}
