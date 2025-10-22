// SPDX-License-Identifier: MIT OR Apache-2.0

//! API for managing members of a group in the shared auth context.
//!
//! Group membership changes also effect all spaces and groups for which the altered group is itself a member.
use std::fmt::Debug;

use p2panda_auth::Access;
use p2panda_auth::traits::{Conditions, Operation};
use p2panda_core::PrivateKey;
use p2panda_encryption::RngError;
use thiserror::Error;

use crate::OperationId;
use crate::auth::message::AuthMessage;
use crate::event::{Event, auth_message_to_group_event};
use crate::identity::IdentityError;
use crate::manager::Manager;
use crate::message::SpacesArgs;
use crate::traits::SpaceId;
use crate::traits::{
    AuthStore, AuthoredMessage, Forge, KeyRegistryStore, KeySecretStore, MessageStore,
    SpacesMessage, SpacesStore,
};
use crate::types::{
    ActorId, AuthControlMessage, AuthGroup, AuthGroupAction, AuthGroupError, AuthGroupState,
    AuthResolver, EncryptionGroupError,
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
pub struct Group<ID, S, K, F, M, C, RS> {
    /// Reference to the manager.
    ///
    /// This allows us to build an API where users can treat "group" instances independently from the
    /// manager API, even though internally it has a reference to it.
    manager: Manager<ID, S, K, F, M, C, RS>,

    /// Id of the group.
    ///
    /// This is the "pointer" at the related group state which lives inside the manager.
    id: ActorId,
}

impl<ID, S, K, F, M, C, RS> Group<ID, S, K, F, M, C, RS>
where
    ID: SpaceId,
    S: SpacesStore<ID, M, C> + AuthStore<C> + MessageStore<M> + Debug,
    K: KeyRegistryStore + KeySecretStore + Debug,
    F: Forge<ID, M, C> + Debug,
    M: AuthoredMessage + SpacesMessage<ID, C> + Debug,
    C: Conditions,
    RS: AuthResolver<C> + Debug,
{
    pub(crate) fn new(manager_ref: Manager<ID, S, K, F, M, C, RS>, id: ActorId) -> Self {
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
    /// Returns messages for replication to other instances and events which inform users of any
    /// state changes which occurred.
    pub(crate) async fn create(
        manager_ref: Manager<ID, S, K, F, M, C, RS>,
        initial_members: Vec<(ActorId, Access<C>)>,
    ) -> Result<(Self, Vec<M>, Event<ID, C>), GroupError<ID, S, K, F, M, C, RS>> {
        // Generate random group id.
        let group_id: ActorId = {
            let manager = manager_ref.inner.read().await;
            let private_key = PrivateKey::from_bytes(&manager.rng.random_array()?);
            private_key.public_key().into()
        };

        let initial_members = typed_members(manager_ref.clone(), initial_members)
            .await
            .map_err(GroupError::AuthStore)?;

        let control_message = AuthControlMessage {
            group_id,
            action: AuthGroupAction::Create {
                initial_members: initial_members.clone(),
            },
        };

        let (messages, mut events) =
            Self::process_local_control(manager_ref.clone(), control_message).await?;

        // Sanity check: there should only one event as this group was only just
        // created and cannot be associated with any space yet.
        assert_eq!(events.len(), 1);
        let event = events.remove(0);

        Ok((
            Self {
                id: group_id,
                manager: manager_ref,
            },
            messages,
            event,
        ))
    }

    /// Add member to group with specified access level.
    ///
    /// Returns messages for replication to other instances and events which inform users of any
    /// state changes which occurred.
    pub async fn add(
        &self,
        member: ActorId,
        access: Access<C>,
    ) -> Result<(Vec<M>, Vec<Event<ID, C>>), GroupError<ID, S, K, F, M, C, RS>> {
        let member = {
            let manager = self.manager.inner.read().await;
            let auth_y = manager.store.auth().await.map_err(GroupError::AuthStore)?;
            typed_member(&auth_y, member)
        };

        let control_message = AuthControlMessage {
            group_id: self.id,
            action: AuthGroupAction::Add { member, access },
        };

        let (messages, events) =
            Self::process_local_control(self.manager.clone(), control_message).await?;

        Ok((messages, events))
    }

    /// Remove member from group.
    ///
    /// Returns messages for replication to other instances and events which inform users of any
    /// state changes which occurred.
    pub async fn remove(
        &self,
        member: ActorId,
    ) -> Result<(Vec<M>, Vec<Event<ID, C>>), GroupError<ID, S, K, F, M, C, RS>> {
        let member = {
            let manager = self.manager.inner.read().await;
            let auth_y = manager.store.auth().await.map_err(GroupError::AuthStore)?;
            typed_member(&auth_y, member)
        };

        let control_message = AuthControlMessage {
            group_id: self.id,
            action: AuthGroupAction::Remove { member },
        };

        let (messages, events) =
            Self::process_local_control(self.manager.clone(), control_message).await?;

        Ok((messages, events))
    }

    /// Process a remote message.
    ///
    /// Returns events which inform users of any state changes which occurred.
    pub(crate) async fn process(
        manager_ref: Manager<ID, S, K, F, M, C, RS>,
        message: &M,
    ) -> Result<Option<Event<ID, C>>, GroupError<ID, S, K, F, M, C, RS>> {
        let auth_message = AuthMessage::from_forged(message);

        let mut auth_y = {
            let manager = manager_ref.inner.read().await;
            manager.store.auth().await.map_err(GroupError::AuthStore)?
        };

        // If we already processed this auth message then return now.
        if auth_y.inner.operations.contains_key(&auth_message.id()) {
            return Ok(None);
        }

        let manager = manager_ref.inner.write().await;
        auth_y = AuthGroup::process(auth_y, &auth_message).map_err(GroupError::AuthGroup)?;
        auth_y
            .orderer_y
            .add_dependency(message.id(), &auth_message.dependencies());
        manager
            .store
            .set_auth(&auth_y)
            .await
            .map_err(GroupError::AuthStore)?;

        Ok(Some(auth_message_to_group_event(&auth_y, &auth_message)))
    }

    /// Process a local control message.
    async fn process_local_control(
        manager_ref: Manager<ID, S, K, F, M, C, RS>,
        control_message: AuthControlMessage<C>,
    ) -> Result<(Vec<M>, Vec<Event<ID, C>>), GroupError<ID, S, K, F, M, C, RS>> {
        let auth_y = {
            let manager = manager_ref.inner.read().await;
            manager.store.auth().await.map_err(GroupError::AuthStore)?
        };

        let (mut auth_y, auth_message) =
            AuthGroup::prepare(auth_y, &control_message).map_err(GroupError::AuthGroup)?;

        let args = SpacesArgs::Auth {
            control_message: auth_message.payload(),
            auth_dependencies: auth_message.dependencies(),
        };

        let message = {
            let mut manager = manager_ref.inner.write().await;
            manager.identity.forge(args).await?
        };
        let auth_message = AuthMessage::from_forged(&message);

        {
            let manager = manager_ref.inner.write().await;
            auth_y = AuthGroup::process(auth_y, &auth_message).map_err(GroupError::AuthGroup)?;
            auth_y
                .orderer_y
                .add_dependency(auth_message.id(), &auth_message.dependencies());
            manager
                .store
                .set_auth(&auth_y)
                .await
                .map_err(GroupError::AuthStore)?;
        }

        let auth_event = auth_message_to_group_event(&auth_y, &auth_message);
        let (space_messages, space_events) = manager_ref
            .apply_group_change_to_spaces(&message)
            .await
            .map_err(|err| GroupError::SyncSpaces(auth_message.id(), format!("{err:?}")))?;

        let mut messages = vec![message];
        let mut events = vec![auth_event];
        messages.extend(space_messages);
        events.extend(space_events);

        Ok((messages, events))
    }

    /// Get the global auth state.
    async fn state(&self) -> Result<AuthGroupState<C>, GroupError<ID, S, K, F, M, C, RS>> {
        let manager = self.manager.inner.read().await;
        let auth_y = manager.store.auth().await.map_err(GroupError::AuthStore)?;
        Ok(auth_y)
    }

    /// Id of this group.
    pub fn id(&self) -> ActorId {
        self.id
    }

    /// Current group members and access levels.
    pub async fn members(
        &self,
    ) -> Result<Vec<(ActorId, Access<C>)>, GroupError<ID, S, K, F, M, C, RS>> {
        let y = self.state().await?;
        let mut group_members = y.members(self.id);
        sort_members(&mut group_members);
        Ok(group_members)
    }
}

/// Group error type.
#[derive(Debug, Error)]
pub enum GroupError<ID, S, K, F, M, C, RS>
where
    ID: SpaceId,
    S: SpacesStore<ID, M, C> + AuthStore<C> + MessageStore<M>,
    K: KeyRegistryStore + KeySecretStore,
    F: Forge<ID, M, C>,
    C: Conditions,
    RS: AuthResolver<C> + Debug,
{
    #[error(transparent)]
    Rng(#[from] RngError),

    #[error("{0}")]
    AuthGroup(AuthGroupError<C, RS>),

    #[error("{0}")]
    EncryptionGroup(EncryptionGroupError<M>),

    #[error(transparent)]
    IdentityManager(#[from] IdentityError<ID, K, F, M, C>),

    #[error("{0}")]
    AuthStore(<S as AuthStore<C>>::Error),

    #[error("{0}")]
    MessageStore(<S as MessageStore<M>>::Error),

    // @TODO: We lose the concrete error type which caused sync of spaces to fail, ideal we would
    // retain this type information.
    #[error("error syncing group change {0} with local spaces: {1}")]
    SyncSpaces(OperationId, String),
}
