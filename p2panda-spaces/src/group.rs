// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fmt::Debug;

use p2panda_auth::Access;
use p2panda_auth::traits::{Conditions, Operation};
use p2panda_core::PrivateKey;
use p2panda_encryption::RngError;
use thiserror::Error;

use crate::OperationId;
use crate::auth::message::AuthMessage;
use crate::event::Event;
use crate::forge::Forge;
use crate::manager::Manager;
use crate::message::{AuthoredMessage, SpacesArgs, SpacesMessage};
use crate::store::{AuthStore, KeyStore, MessageStore, SpaceStore};
use crate::traits::SpaceId;
use crate::types::{
    ActorId, AuthControlMessage, AuthGroup, AuthGroupAction, AuthGroupError, AuthGroupState,
    AuthResolver, EncryptionGroupError,
};
use crate::utils::{typed_member, typed_members};

#[derive(Debug)]
pub struct Group<ID, S, F, M, C, RS> {
    /// Reference to the manager.
    ///
    /// This allows us build an API where users can treat "group" instances independently from the
    /// manager API, even though internally it has a reference to it.
    manager: Manager<ID, S, F, M, C, RS>,

    /// Id of the group.
    ///
    /// This is the "pointer" at the related group state which lives inside the manager.
    id: ActorId,
}

impl<ID, S, F, M, C, RS> Group<ID, S, F, M, C, RS>
where
    ID: SpaceId,
    S: SpaceStore<ID, M, C> + KeyStore + AuthStore<C> + MessageStore<M> + Debug,
    F: Forge<ID, M, C> + Debug,
    M: AuthoredMessage + SpacesMessage<ID, C>,
    C: Conditions,
    RS: Debug + AuthResolver<C>,
{
    pub(crate) fn new(manager_ref: Manager<ID, S, F, M, C, RS>, id: ActorId) -> Self {
        Self {
            manager: manager_ref,
            id,
        }
    }

    /// Create a group containing initial members with associated access levels.
    ///
    /// It is possible to create a group where the creator is not an initial member or is a member
    /// without manager rights. If this is done then after creation no further change of the group
    /// membership would be possible.
    pub(crate) async fn create(
        manager_ref: Manager<ID, S, F, M, C, RS>,
        initial_members: Vec<(ActorId, Access<C>)>,
    ) -> Result<(Self, Vec<M>), GroupError<ID, S, F, M, C, RS>> {
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

        let auth_message =
            Self::process_local_control(manager_ref.clone(), control_message).await?;
        let space_messages = manager_ref
            .sync_spaces(&auth_message)
            .await
            .map_err(|err| GroupError::SyncSpaces(auth_message.id(), format!("{err:?}")))?;

        let mut messages = vec![auth_message];
        messages.extend(space_messages);

        Ok((
            Self {
                id: group_id,
                manager: manager_ref,
            },
            messages,
        ))
    }

    /// Add member to an existing group with specified access level.
    pub async fn add(
        &self,
        member: ActorId,
        access: Access<C>,
    ) -> Result<Vec<M>, GroupError<ID, S, F, M, C, RS>> {
        let member = {
            let manager = self.manager.inner.read().await;
            let auth_y = manager.store.auth().await.map_err(GroupError::AuthStore)?;
            typed_member(&auth_y, member)
        };

        let control_message = AuthControlMessage {
            group_id: self.id,
            action: AuthGroupAction::Add { member, access },
        };

        let auth_message =
            Self::process_local_control(self.manager.clone(), control_message).await?;
        let space_messages = self
            .manager
            .sync_spaces(&auth_message)
            .await
            .map_err(|err| GroupError::SyncSpaces(auth_message.id(), format!("{err:?}")))?;

        let mut messages = vec![auth_message];
        messages.extend(space_messages);

        Ok(messages)
    }

    /// Remove member from an existing group.
    pub async fn remove(&self, member: ActorId) -> Result<Vec<M>, GroupError<ID, S, F, M, C, RS>> {
        let member = {
            let manager = self.manager.inner.read().await;
            let auth_y = manager.store.auth().await.map_err(GroupError::AuthStore)?;
            typed_member(&auth_y, member)
        };

        let control_message = AuthControlMessage {
            group_id: self.id,
            action: AuthGroupAction::Remove { member },
        };

        let auth_message =
            Self::process_local_control(self.manager.clone(), control_message).await?;
        let space_messages = self
            .manager
            .sync_spaces(&auth_message)
            .await
            .map_err(|err| GroupError::SyncSpaces(auth_message.id(), format!("{err:?}")))?;

        let mut messages = vec![auth_message];
        messages.extend(space_messages);

        Ok(messages)
    }

    /// Process a remote message.
    pub(crate) async fn process(
        manager_ref: Manager<ID, S, F, M, C, RS>,
        message: &M,
    ) -> Result<Vec<Event<ID>>, GroupError<ID, S, F, M, C, RS>> {
        let auth_message = AuthMessage::from_forged(message);

        let mut auth_y = {
            let manager = manager_ref.inner.read().await;
            manager.store.auth().await.map_err(GroupError::AuthStore)?
        };

        let mut manager = manager_ref.inner.write().await;
        auth_y = AuthGroup::process(auth_y, &auth_message).map_err(GroupError::AuthGroup)?;
        auth_y
            .orderer_y
            .add_dependency(message.id(), &auth_message.dependencies());
        manager
            .store
            .set_auth(&auth_y)
            .await
            .map_err(GroupError::AuthStore)?;
        manager
            .store
            .set_message(&message.id(), message)
            .await
            .map_err(GroupError::MessageStore)?;

        Ok(vec![])
    }

    /// Process a local control message.
    async fn process_local_control(
        manager_ref: Manager<ID, S, F, M, C, RS>,
        control_message: AuthControlMessage<C>,
    ) -> Result<M, GroupError<ID, S, F, M, C, RS>> {
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
            let message = manager.forge.forge(args).await.map_err(GroupError::Forge)?;
            manager
                .store
                .set_message(&message.id(), &message)
                .await
                .map_err(GroupError::MessageStore)?;
            message
        };

        {
            let mut manager = manager_ref.inner.write().await;
            let auth_message = AuthMessage::from_forged(&message);
            auth_y = AuthGroup::process(auth_y, &auth_message).map_err(GroupError::AuthGroup)?;
            auth_y
                .orderer_y
                .add_dependency(message.id(), &auth_message.dependencies());
            manager
                .store
                .set_auth(&auth_y)
                .await
                .map_err(GroupError::AuthStore)?;
        }

        Ok(message)
    }

    /// Get the global auth state.
    async fn state(&self) -> Result<AuthGroupState<C>, GroupError<ID, S, F, M, C, RS>> {
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
    ) -> Result<Vec<(ActorId, Access<C>)>, GroupError<ID, S, F, M, C, RS>> {
        let y = self.state().await?;
        let group_members = y.members(self.id);
        Ok(group_members)
    }
}

#[derive(Debug, Error)]
pub enum GroupError<ID, S, F, M, C, RS>
where
    ID: SpaceId,
    S: SpaceStore<ID, M, C> + KeyStore + AuthStore<C> + MessageStore<M>,
    F: Forge<ID, M, C>,
    C: Conditions,
    RS: Debug + AuthResolver<C>,
{
    #[error(transparent)]
    Rng(#[from] RngError),

    #[error("{0}")]
    AuthGroup(AuthGroupError<C, RS>),

    #[error("{0}")]
    EncryptionGroup(EncryptionGroupError<M>),

    #[error("{0}")]
    Forge(F::Error),

    #[error("{0}")]
    AuthStore(<S as AuthStore<C>>::Error),

    #[error("{0}")]
    MessageStore(<S as MessageStore<M>>::Error),

    // @TODO: We lose the concrete error type which caused sync of spaces to fail, ideal we would
    // retain this type information.
    #[error("error syncing group change {0} with local spaces: {1}")]
    SyncSpaces(OperationId, String),
}
