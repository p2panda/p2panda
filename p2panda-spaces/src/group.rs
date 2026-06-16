// SPDX-License-Identifier: MIT OR Apache-2.0

//! API for managing members of a group in the shared auth context.
//!
//! Group membership changes also effect all spaces and groups for which the altered group is itself a member.
use std::borrow::Borrow;
use std::fmt::Debug;

use p2panda_auth::Access;
use p2panda_auth::group::GroupAction;
use p2panda_auth::traits::{Conditions, Operation};
use p2panda_encryption::RngError;
use p2panda_encryption::key_registry::KeyRegistryState;
use p2panda_store::key_registry::KeyRegistryStore;
use thiserror::Error;

use crate::OperationId;
use crate::auth::message::AuthMessage;
use crate::event::{Event, auth_message_to_group_event};
use crate::identity::IdentityError;
use crate::manager::Manager;
use crate::message::{SpacesArgs, SpacesMessage};
use crate::traits::{
    AuthStore, AuthoredMessage, Forge, KeySecretStore, MessageStore, SpaceId, SpacesStore,
};
use crate::types::{
    ActorId, AuthGroup, AuthGroupAction, AuthGroupError, AuthGroupState, AuthResolver,
    EncryptionGroupError,
};
use crate::utils::{sort_members, typed_member, typed_members};

/// A single group which exists in the global auth context.
///
/// Actors can be added or removed from the group; an actor may be an individual or a group.
/// Access levels are assigned to all members. These access levels can be used by authorisation
/// layers outside of p2panda-spaces to enforce access control rules.
///
/// A group can be a member of many spaces, or indeed other groups, and any changes effect all
/// parents.
///
/// Only members with Manage access level are allowed to manage the groups members.
#[derive(Debug)]
pub struct Group<ID, S, K, F, C, RS> {
    /// Reference to the manager.
    ///
    /// This allows us to build an API where users can treat "group" instances independently from the
    /// manager API, even though internally it has a reference to it.
    manager: Manager<ID, S, K, F, C, RS>,

    /// Id of the group.
    ///
    /// This is the "pointer" at the related group state which lives inside the manager.
    id: ActorId,
}

impl<ID, S, K, F, C, RS> Group<ID, S, K, F, C, RS>
where
    ID: SpaceId,
    S: SpacesStore<ID, C> + AuthStore<C> + MessageStore<F::Message> + Debug,
    K: KeyRegistryStore<KeyRegistryState<ActorId>> + KeySecretStore + Debug,
    F: Forge<ID, C> + Debug,
    F::Message: AuthoredMessage + Borrow<SpacesArgs<ID, C>>,
    C: Conditions,
    RS: AuthResolver<C> + Debug,
{
    pub(crate) fn new(manager_ref: Manager<ID, S, K, F, C, RS>, id: ActorId) -> Self {
        Self {
            manager: manager_ref,
            id,
        }
    }

    /// Create a group containing initial members with associated access levels.
    ///
    /// It is possible to create a group where the creator is not an initial member or is a member
    /// without manager rights. If this is done then after creation no further change of the group
    /// membership would be possible by the local actor.
    ///
    /// Returns resulting state and message for processing.
    pub(crate) async fn create(
        manager_ref: Manager<ID, S, K, F, C, RS>,
        y: AuthGroupState<C>,
        group_id: ActorId,
        initial_members: Vec<(ActorId, Access<C>)>,
    ) -> Result<(AuthGroupState<C>, F::Message), GroupError<ID, S, K, F, C, RS>> {
        let initial_members = typed_members(&y, initial_members);

        let auth_dependencies = y.inner.heads().into_iter().collect();
        let action = AuthGroupAction::Create {
            initial_members: initial_members.clone(),
        };

        let (y, message) = Self::process_local_control(
            manager_ref.clone(),
            y,
            group_id,
            auth_dependencies,
            action,
        )
        .await?;

        Ok((y, message))
    }

    /// Add member to group with specified access level.
    ///
    /// Persists resulting state and returns forged message.
    #[cfg(any(test, feature = "test_utils"))]
    pub async fn add_persisted(
        &self,
        member: ActorId,
        access: Access<C>,
    ) -> Result<F::Message, GroupError<ID, S, K, F, C, RS>> {
        let (y, message) = self.add(member, access).await?;
        self.manager
            .set_auth(&y)
            .await
            .map_err(GroupError::AuthStore)?;

        Ok(message)
    }

    /// Add member to group with specified access level.
    ///
    /// Returns resulting state and message for processing.
    pub async fn add(
        &self,
        member: ActorId,
        access: Access<C>,
    ) -> Result<(AuthGroupState<C>, F::Message), GroupError<ID, S, K, F, C, RS>> {
        let y = self.manager.auth().await.map_err(GroupError::AuthStore)?;

        let member = typed_member(&y, member);
        let dependencies = y.inner.heads().into_iter().collect();
        let action = AuthGroupAction::Add { member, access };

        Self::process_local_control(self.manager.clone(), y, self.id, dependencies, action).await
    }

    /// Remove member from group.
    ///
    /// Persists resulting state and returns forged message.
    #[cfg(any(test, feature = "test_utils"))]
    pub async fn remove_persisted(
        &self,
        member: ActorId,
    ) -> Result<F::Message, GroupError<ID, S, K, F, C, RS>> {
        let (y, message) = self.remove(member).await?;
        self.manager
            .set_auth(&y)
            .await
            .map_err(GroupError::AuthStore)?;

        Ok(message)
    }

    /// Remove member from group.
    ///
    /// Returns resulting state and message for processing.
    pub async fn remove(
        &self,
        member: ActorId,
    ) -> Result<(AuthGroupState<C>, F::Message), GroupError<ID, S, K, F, C, RS>> {
        let y = self.manager.auth().await.map_err(GroupError::AuthStore)?;

        let member = typed_member(&y, member);
        let dependencies = y.inner.heads().into_iter().collect();
        let action = AuthGroupAction::Remove { member };

        Self::process_local_control(self.manager.clone(), y, self.id, dependencies, action).await
    }

    /// Process a remote message.
    ///
    /// Returns events which inform users of any state changes which occurred.
    pub(crate) async fn process(
        manager_ref: Manager<ID, S, K, F, C, RS>,
        auth_message: &AuthMessage<C>,
    ) -> Result<Option<Event<ID, C>>, GroupError<ID, S, K, F, C, RS>> {
        let mut auth_y = {
            let manager = manager_ref.inner.read().await;
            manager.store.auth().await.map_err(GroupError::AuthStore)?
        };

        // If we already processed this auth message then return now.
        if auth_y.inner.operations.contains_key(&auth_message.id()) {
            return Ok(None);
        }

        let manager = manager_ref.inner.write().await;
        auth_y = AuthGroup::process(auth_y, auth_message).map_err(GroupError::AuthGroup)?;
        manager
            .store
            .set_auth(&auth_y)
            .await
            .map_err(GroupError::AuthStore)?;

        Ok(Some(auth_message_to_group_event(&auth_y, auth_message)))
    }

    /// Process a local control message.
    pub async fn process_local_control(
        manager_ref: Manager<ID, S, K, F, C, RS>,
        y: AuthGroupState<C>,
        group_id: ActorId,
        auth_dependencies: Vec<OperationId>,
        group_action: GroupAction<ActorId, C>,
    ) -> Result<(AuthGroupState<C>, F::Message), GroupError<ID, S, K, F, C, RS>> {
        let args = SpacesArgs::Auth {
            group_id,
            auth_dependencies,
            group_action,
        };

        let message = {
            let mut manager = manager_ref.inner.write().await;
            manager.identity.forge(args).await?
        };

        let y =
            AuthGroup::process(y, &SpacesMessage::auth(&message)).map_err(GroupError::AuthGroup)?;

        Ok((y, message))
    }

    /// Id of this group.
    pub fn id(&self) -> ActorId {
        self.id
    }

    /// Current group members and access levels.
    pub async fn members(
        &self,
    ) -> Result<Vec<(ActorId, Access<C>)>, GroupError<ID, S, K, F, C, RS>> {
        let y = self.manager.auth().await.map_err(GroupError::AuthStore)?;
        let mut group_members = y.members(self.id);
        sort_members(&mut group_members);
        Ok(group_members)
    }
}

/// Group error type.
#[derive(Debug, Error)]
pub enum GroupError<ID, S, K, F, C, RS>
where
    ID: SpaceId,
    S: SpacesStore<ID, C> + AuthStore<C> + MessageStore<F::Message>,
    K: KeyRegistryStore<KeyRegistryState<ActorId>> + KeySecretStore,
    F: Forge<ID, C>,
    C: Conditions,
    RS: AuthResolver<C> + Debug,
{
    #[error(transparent)]
    Rng(#[from] RngError),

    #[error("{0}")]
    AuthGroup(AuthGroupError<C, RS>),

    #[error("{0}")]
    EncryptionGroup(EncryptionGroupError),

    #[error(transparent)]
    IdentityManager(#[from] IdentityError<ID, K, F, C>),

    #[error("{0}")]
    AuthStore(<S as AuthStore<C>>::Error),

    #[error("{0}")]
    MessageStore(<S as MessageStore<F::Message>>::Error),

    // @TODO: We lose the concrete error type which caused sync of spaces to fail, ideal we would
    // retain this type information.
    #[error("error syncing group change {0} with local spaces: {1}")]
    SyncSpaces(OperationId, String),
}
