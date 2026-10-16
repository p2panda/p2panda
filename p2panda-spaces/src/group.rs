// SPDX-License-Identifier: MIT OR Apache-2.0

//! API for managing members of a group in the shared auth context.
//!
//! Group membership changes also effect all spaces and groups for which the altered group is itself a member.
use std::fmt::Debug;

use p2panda_auth::Access;
use p2panda_auth::group::GroupAction;
use p2panda_auth::traits::{Conditions, Operation};
use p2panda_core::VerifyingKey;
use p2panda_core::traits::ShortFormat;
use p2panda_store::Transaction;
use p2panda_store::groups::GroupsStore;
use p2panda_store::key_registry::KeyRegistryStore;
use p2panda_store::key_secrets::PreKeyBundlesStore;
use p2panda_store::spaces::{SpacesMessageStore, SpacesStore};
use thiserror::Error;
use tracing::debug;

use crate::event::{Event, to_groups_event};
use crate::forge::Forge;
use crate::identity::IdentityError;
use crate::manager::{Manager, StoreError};
use crate::message::{AuthMessage, SpacesArgs, SpacesMessage};
use crate::store::{Context, SpacesStoreState, StateChanges, StateError};
use crate::types::{AuthGroup, AuthGroupAction, AuthGroupError, AuthGroupState};
use crate::utils::{sort_members, typed_member, typed_members};
use crate::{ActorId, GroupId, MemberId};

/// A single group which exists in the global auth context.
///
/// Actors can be added or removed from the group; an actor may be an individual or a group.
/// Access levels are assigned to all members. These access levels can be used by authorisation
/// layers outside of p2panda-spaces to enforce access control rules.
///
/// A group can be a member of many spaces, or indeed other groups, and any changes effect all
/// ancestors.
///
/// Only members with Manage access level are allowed to manage the groups members.
#[derive(Debug)]
pub struct Group<S, F, C> {
    /// Reference to the manager.
    ///
    /// This allows us to build an API where users can treat "group" instances independently from the
    /// manager API, even though internally it has a reference to it.
    manager: Manager<S, F, C>,

    /// Id of the group.
    ///
    /// This is the "pointer" at the related group state which lives inside the manager.
    id: GroupId,
}

/// Output from methods which create a group or change the membership of an existing group.
#[derive(Debug)]
pub struct GroupOutput<C, M> {
    pub group_id: GroupId,
    pub message: M,
    pub event: Event<C>,
}

impl<C, M> GroupOutput<C, M>
where
    C: Conditions,
{
    pub fn from(group_id: GroupId, message: M, event: Event<C>) -> Self {
        Self {
            group_id,
            message,
            event,
        }
    }
}

impl<S, F, C> Group<S, F, C>
where
    S: Clone
        + SpacesStore<SpacesStoreState<C>>
        + SpacesMessageStore<F::Message>
        + GroupsStore<AuthMessage<C>, C>
        + KeyRegistryStore
        + PreKeyBundlesStore
        + Transaction,
    F: Forge<C>,
    C: Conditions,
{
    pub(crate) fn new(manager_ref: Manager<S, F, C>, id: GroupId) -> Self {
        Self {
            manager: manager_ref,
            id,
        }
    }

    /// Verifying key of the local actor.
    pub fn my_id(&self) -> VerifyingKey {
        self.manager.id()
    }

    /// Create a group containing initial members with associated access levels.
    ///
    /// It is possible to create a group where the creator is not an initial member or is a member
    /// without manager rights. If this is done then after creation no further change of the group
    /// membership would be possible by the local actor.
    ///
    /// Returns resulting state and message for processing.
    pub(crate) async fn create(
        ctx: &Context<'_, S, F>,
        global_groups_y: AuthGroupState<C>,
        group_id: GroupId,
        initial_members: Vec<(ActorId, Access<C>)>,
    ) -> Result<(AuthGroupState<C>, GroupOutput<C, F::Message>), GroupError<F, C>> {
        let initial_members = typed_members(&global_groups_y, &initial_members);
        let action = AuthGroupAction::Create { initial_members };
        Self::apply(ctx, global_groups_y, group_id, action).await
    }

    /// Add member to group with specified access level.
    ///
    /// Persists resulting state and returns forged message.
    pub async fn add(
        &self,
        member: ActorId,
        access: Access<C>,
    ) -> Result<GroupOutput<C, F::Message>, GroupError<F, C>> {
        self.apply_and_commit(|y| AuthGroupAction::Add {
            member: typed_member(y, member),
            access,
        })
        .await
    }

    /// Remove member from group.
    ///
    /// Persists resulting state and returns forged message.
    pub async fn remove(
        &self,
        member: ActorId,
    ) -> Result<GroupOutput<C, F::Message>, GroupError<F, C>> {
        self.apply_and_commit(|y| AuthGroupAction::Remove {
            member: typed_member(y, member),
        })
        .await
    }

    /// Promote an existing group member to specified access level.
    ///
    /// Persists resulting state and returns forged message.
    pub async fn promote(
        &self,
        member: ActorId,
        access: Access<C>,
    ) -> Result<GroupOutput<C, F::Message>, GroupError<F, C>> {
        self.apply_and_commit(|y| AuthGroupAction::Promote {
            member: typed_member(y, member),
            access,
        })
        .await
    }

    /// Demote an existing group member to specified access level.
    ///
    /// Persists resulting state and returns forged message.
    pub async fn demote(
        &self,
        member: ActorId,
        access: Access<C>,
    ) -> Result<GroupOutput<C, F::Message>, GroupError<F, C>> {
        self.apply_and_commit(|y| AuthGroupAction::Demote {
            member: typed_member(y, member),
            access,
        })
        .await
    }

    /// Apply an action on this group in its own transaction and commit the resulting global groups
    /// state.
    ///
    /// The action is constructed from the global groups state read within the transaction.
    async fn apply_and_commit(
        &self,
        action: impl FnOnce(&AuthGroupState<C>) -> AuthGroupAction<C>,
    ) -> Result<GroupOutput<C, F::Message>, GroupError<F, C>> {
        let tx = self.manager.begin_write().await?;

        // Read.

        let global_groups_y = tx.global_groups_state().await?;

        // Modify.

        let action = action(&global_groups_y);
        let (global_groups_y, output) =
            Self::apply(&tx.ctx, global_groups_y, self.id, action).await?;

        // Write.

        tx.commit(StateChanges::GlobalGroups { global_groups_y })
            .await?;

        self.manager.waiters().notify_events([&output.event]);

        Ok(output)
    }

    /// Process a remote message.
    ///
    /// Returns events which inform users of any state changes which occurred. The returned state
    /// is `None` if the message was already processed before and nothing changed.
    #[allow(clippy::type_complexity)]
    pub(crate) fn process(
        global_groups_y: AuthGroupState<C>,
        auth_message: &AuthMessage<C>,
    ) -> Result<(Option<AuthGroupState<C>>, Event<C>), GroupError<F, C>> {
        let previous_ancestors = global_groups_y.inner.ancestors(auth_message.group_id());
        let has_seen = global_groups_y
            .inner
            .operations
            .contains_key(&auth_message.id());

        if has_seen {
            debug!(
                message_id = auth_message.id().fmt_short(),
                "ignore already processed auth groups message"
            );
            let event = to_groups_event(&global_groups_y, auth_message, &previous_ancestors);
            return Ok((None, event));
        }

        let global_groups_y = AuthGroup::<C>::process(global_groups_y, auth_message)
            .map_err(GroupError::AuthGroup)?;
        let event = to_groups_event(&global_groups_y, auth_message, &previous_ancestors);

        Ok((Some(global_groups_y), event))
    }

    /// Apply a locally authored action on a group in the global groups state.
    ///
    /// Forges the auth message within the currently open transaction and returns the resulting
    /// global groups state, without persisting it.
    pub(crate) async fn apply(
        ctx: &Context<'_, S, F>,
        global_groups_y: AuthGroupState<C>,
        group_id: GroupId,
        action: GroupAction<ActorId, C>,
    ) -> Result<(AuthGroupState<C>, GroupOutput<C, F::Message>), GroupError<F, C>> {
        // Compute the auth graph heads to include as dependencies based on the groups included in
        // this action. This means any groups being added / removed in the action, plus the id of
        // the ancestor group itself.
        let dependencies = AuthGroup::heads(&global_groups_y, group_id, &action);

        let args = SpacesArgs::Group {
            group_id,
            auth_dependencies: dependencies,
            group_action: action,
        };

        let message = ctx.forge(args).await?;

        let group_message = SpacesMessage::auth(&message);
        let previous_ancestors = global_groups_y.inner.ancestors(group_id);
        let global_groups_y = AuthGroup::<C>::process(global_groups_y, &group_message)
            .map_err(GroupError::AuthGroup)?;
        let event = to_groups_event(&global_groups_y, &group_message, &previous_ancestors);

        Ok((global_groups_y, GroupOutput::from(group_id, message, event)))
    }

    /// Id of this group.
    pub fn id(&self) -> GroupId {
        self.id
    }

    /// Current group members and access levels.
    pub async fn members(&self) -> Result<Vec<(MemberId, Access<C>)>, GroupError<F, C>> {
        let y = self.manager.read().global_groups_state().await?;
        let mut group_members = y.members(self.id);
        sort_members(&mut group_members);
        Ok(group_members)
    }

    /// All actors (both groups and individuals) in the group.
    pub async fn actors(&self) -> Result<Vec<(MemberId, Access<C>)>, GroupError<F, C>> {
        let y = self.manager.read().global_groups_state().await?;
        let mut members: Vec<(MemberId, Access<C>)> = y
            .root_members(self.id)
            .into_iter()
            .map(|(member, access)| (member.id(), access))
            .collect();
        sort_members(&mut members);
        Ok(members)
    }
}

/// Group error type.
#[derive(Debug, Error)]
pub enum GroupError<F, C>
where
    F: Forge<C>,
    C: Conditions,
{
    #[error("{0}")]
    AuthGroup(AuthGroupError),

    #[error(transparent)]
    IdentityManager(#[from] IdentityError<F, C>),

    #[error(transparent)]
    Store(#[from] StoreError),

    #[error(transparent)]
    State(#[from] StateError),
}
