// SPDX-License-Identifier: MIT OR Apache-2.0

//! API for managing members of a group in the shared auth context.
//!
//! Group membership changes also effect all spaces and groups for which the altered group is itself a member.
use std::fmt::Debug;

use p2panda_auth::Access;
use p2panda_auth::group::GroupAction;
use p2panda_auth::traits::{Conditions, Operation};
use p2panda_core::{Hash, VerifyingKey};
use p2panda_encryption::RngError;
use p2panda_store::Transaction;
use p2panda_store::groups::GroupsStore;
use p2panda_store::key_registry::KeyRegistryStore;
use p2panda_store::key_secrets::KeySecretsStore;
use p2panda_store::spaces::{SpacesMessageStore, SpacesStore, SpacesStoreWrite};
use thiserror::Error;

use crate::StoreError;
use crate::auth::message::AuthMessage;
use crate::event::{Event, auth_message_to_group_event};
use crate::forge::Forge;
use crate::identity::IdentityError;
use crate::manager::Manager;
use crate::message::{SpacesArgs, SpacesMessage};
use crate::space::SpaceState;
use crate::types::{
    AuthGroup, AuthGroupAction, AuthGroupError, AuthGroupState, AuthResolver, EncryptionGroupError,
};
use crate::utils::{sort_members, typed_member, typed_members};

pub const GROUPS_STORE_STATE_ID: &[u8] = b"global-groups-context";

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
pub struct Group<S, F, C, RS> {
    /// Reference to the manager.
    ///
    /// This allows us to build an API where users can treat "group" instances independently from the
    /// manager API, even though internally it has a reference to it.
    manager: Manager<S, F, C, RS>,

    /// Id of the group.
    ///
    /// This is the "pointer" at the related group state which lives inside the manager.
    id: VerifyingKey,
}

impl<S, F, C, RS> Group<S, F, C, RS>
where
    S: Clone
        + Transaction
        + SpacesStore<SpaceState<C>>
        + SpacesStoreWrite<SpaceState<C>>
        + SpacesMessageStore<SpacesArgs<C>>
        + GroupsStore<AuthMessage<C>, C>
        + KeyRegistryStore
        + KeySecretsStore,
    F: Forge<C>,
    C: Conditions,
    RS: AuthResolver<C>,
{
    pub(crate) fn new(manager_ref: Manager<S, F, C, RS>, id: VerifyingKey) -> Self {
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
        manager_ref: Manager<S, F, C, RS>,
        y: AuthGroupState<C>,
        group_id: VerifyingKey,
        initial_members: Vec<(VerifyingKey, Access<C>)>,
    ) -> Result<(AuthGroupState<C>, F::Message), GroupError<F, C, RS>> {
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
        member: VerifyingKey,
        access: Access<C>,
    ) -> Result<F::Message, GroupError<F, C, RS>> {
        let (y, message) = self.add(member, access).await?;

        self.manager
            .set_groups_state(&y)
            .await
            .map_err(|err| StoreError::from(err.to_string()))?;

        Ok(message)
    }

    /// Add member to group with specified access level.
    ///
    /// Returns resulting state and message for processing.
    pub async fn add(
        &self,
        member: VerifyingKey,
        access: Access<C>,
    ) -> Result<(AuthGroupState<C>, F::Message), GroupError<F, C, RS>> {
        let y = self
            .manager
            .groups_state()
            .await
            .map_err(|err| StoreError::from(err.to_string()))?;

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
        member: VerifyingKey,
    ) -> Result<F::Message, GroupError<F, C, RS>> {
        let (y, message) = self.remove(member).await?;
        self.manager
            .set_groups_state(&y)
            .await
            .map_err(|err| StoreError::from(err.to_string()))?;

        Ok(message)
    }

    /// Remove member from group.
    ///
    /// Returns resulting state and message for processing.
    pub async fn remove(
        &self,
        member: VerifyingKey,
    ) -> Result<(AuthGroupState<C>, F::Message), GroupError<F, C, RS>> {
        let y = self.manager.groups_state().await?;

        let member = typed_member(&y, member);
        let dependencies = y.inner.heads().into_iter().collect();
        let action = AuthGroupAction::Remove { member };

        Self::process_local_control(self.manager.clone(), y, self.id, dependencies, action).await
    }

    /// Process a remote message.
    ///
    /// Returns events which inform users of any state changes which occurred.
    pub(crate) async fn process(
        manager_ref: Manager<S, F, C, RS>,
        auth_message: &AuthMessage<C>,
    ) -> Result<Option<Event<C>>, GroupError<F, C, RS>> {
        let mut auth_y = manager_ref.groups_state().await?;

        // If we already processed this auth message then return now.
        if auth_y.inner.operations.contains_key(&auth_message.id()) {
            return Ok(None);
        }

        auth_y = AuthGroup::process(auth_y, auth_message).map_err(GroupError::AuthGroup)?;
        manager_ref
            .set_groups_state(&auth_y)
            .await
            .map_err(|err| StoreError::from(err.to_string()))?;

        Ok(Some(auth_message_to_group_event(&auth_y, auth_message)))
    }

    /// Process a local control message.
    pub async fn process_local_control(
        manager_ref: Manager<S, F, C, RS>,
        y: AuthGroupState<C>,
        group_id: VerifyingKey,
        auth_dependencies: Vec<Hash>,
        group_action: GroupAction<VerifyingKey, C>,
    ) -> Result<(AuthGroupState<C>, F::Message), GroupError<F, C, RS>> {
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
    pub fn id(&self) -> VerifyingKey {
        self.id
    }

    /// Current group members and access levels.
    pub async fn members(&self) -> Result<Vec<(VerifyingKey, Access<C>)>, GroupError<F, C, RS>> {
        let y = self
            .manager
            .groups_state()
            .await
            .map_err(|err| StoreError::from(err.to_string()))?;
        let mut group_members = y.members(self.id);
        sort_members(&mut group_members);
        Ok(group_members)
    }
}

/// Group error type.
#[derive(Debug, Error)]
pub enum GroupError<F, C, RS>
where
    F: Forge<C>,
    C: Conditions,
    RS: AuthResolver<C>,
{
    #[error(transparent)]
    Rng(#[from] RngError),

    #[error("{0}")]
    AuthGroup(AuthGroupError<C, RS>),

    #[error("{0}")]
    EncryptionGroup(EncryptionGroupError),

    #[error(transparent)]
    IdentityManager(#[from] IdentityError<F, C>),

    #[error(transparent)]
    Store(#[from] StoreError),

    // TODO: We lose the concrete error type which caused sync of spaces to fail, ideal we would
    // retain this type information.
    #[error("error syncing group change {0} with local spaces: {1}")]
    SyncSpaces(Hash, String),
}
