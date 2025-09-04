// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashSet;
use std::fmt::Debug;

use p2panda_auth::Access;
use p2panda_auth::group::GroupMember;
use p2panda_auth::traits::{Conditions, Operation};
use p2panda_core::PrivateKey;
use p2panda_encryption::RngError;
use thiserror::Error;

use crate::auth::message::{AuthArgs, AuthMessage};
use crate::auth::orderer::AuthOrdererState;
use crate::encryption::dgm::EncryptionMembershipState;
use crate::encryption::message::EncryptionMessage;
use crate::encryption::orderer::EncryptionOrdererState;
use crate::event::Event;
use crate::forge::Forge;
use crate::manager::Manager;
use crate::message::{AuthoredMessage, SpacesArgs, SpacesMessage};
use crate::store::{AuthStore, KeyStore, SpaceStore};
use crate::types::{
    ActorId, AuthControlMessage, AuthGroup, AuthGroupAction, AuthGroupError, AuthGroupState,
    AuthResolver, EncryptionGroup, EncryptionGroupError, EncryptionGroupOutput,
    EncryptionGroupState, OperationId,
};

/// Encrypted data context with authorization boundary.
///
/// Only members with suitable access to the space can read and write to it.
#[derive(Debug)]
pub struct Space<S, F, M, C, RS> {
    /// Reference to the manager.
    ///
    /// This allows us build an API where users can treat "space" instances independently from the
    /// manager API, even though internally it has a reference to it.
    manager: Manager<S, F, M, C, RS>,

    /// Id of the space.
    ///
    /// This is the "pointer" at the related space state which lives inside the manager.
    id: ActorId,
}

impl<S, F, M, C, RS> Space<S, F, M, C, RS>
where
    S: SpaceStore<M> + KeyStore + AuthStore<C>,
    F: Forge<M, C>,
    M: AuthoredMessage + SpacesMessage<C>,
    C: Conditions,
    RS: Debug + AuthResolver<C>,
{
    pub(crate) fn new(manager_ref: Manager<S, F, M, C, RS>, id: ActorId) -> Self {
        Self {
            manager: manager_ref,
            id,
        }
    }

    #[allow(clippy::result_large_err)]
    pub(crate) async fn create(
        manager_ref: Manager<S, F, M, C, RS>,
        mut initial_members: Vec<(GroupMember<ActorId>, Access<C>)>,
    ) -> Result<(Self, M), SpaceError<S, F, M, C, RS>> {
        let my_id: ActorId = {
            let manager = manager_ref.inner.read().await;
            manager.forge.public_key().into()
        };

        // 1. Derive a space id and initialize new new state.

        let space_id: ActorId = {
            let manager = manager_ref.inner.write().await;
            let private_key = PrivateKey::from_bytes(&manager.rng.random_array()?);
            private_key.public_key().into()
        };
        let mut y = Self::get_or_init_state(space_id, manager_ref.clone()).await?;
        let auth_y = {
            let manager = manager_ref.inner.read().await;
            manager.store.auth().await.map_err(SpaceError::AuthStore)?
        };

        // 2. Prepare auth "create" control message.

        let (mut auth_y, auth_message) = {
            // Automatically add ourselves with "manage" level without any conditions as default.
            if !initial_members
                .iter()
                .any(|(member, _)| member.id() == my_id)
            {
                initial_members.push((GroupMember::Individual(my_id), Access::manage()));
            }

            let auth_control_message = AuthControlMessage {
                group_id: space_id,
                action: AuthGroupAction::Create {
                    initial_members: initial_members.clone(),
                },
            };
            AuthGroup::prepare(auth_y, &auth_control_message).map_err(SpaceError::AuthGroup)?
        };

        // 3. Prepare and process encryption control message(s), establishing initial state.

        let (encryption_y, encryption_message) = Self::process_group_membership_change(
            manager_ref.clone(),
            space_id,
            y.encryption_y,
            &auth_message,
        )
        .await?;
        y.encryption_y = encryption_y;

        // 4. Merge and sign control messages in forge (F).

        let message = Self::forge(
            manager_ref.clone(),
            space_id,
            auth_message,
            encryption_message,
        )
        .await?;

        // 5. Process auth message.

        auth_y = {
            let auth_message = AuthMessage::from_forged(&message);
            AuthGroup::process(auth_y, &auth_message).map_err(SpaceError::AuthGroup)?
        };

        // 6. Update auth and encryption orderer states.

        (y.encryption_y.orderer, auth_y.orderer_y) =
            Self::update_orderer_states(y.encryption_y.orderer, auth_y.orderer_y, &message);

        // 7. Persist new state.

        Self::set_state(manager_ref.clone(), y, auth_y).await?;

        Ok((
            Self {
                id: space_id,
                manager: manager_ref,
            },
            message,
        ))
    }

    #[allow(clippy::result_large_err)]
    pub(crate) async fn add(
        &self,
        member: GroupMember<ActorId>,
        access: Access<C>,
    ) -> Result<M, SpaceError<S, F, M, C, RS>> {
        let mut y = Self::get_or_init_state(self.id(), self.manager.clone()).await?;
        let auth_y = {
            let manager = self.manager.inner.read().await;
            manager.store.auth().await.map_err(SpaceError::AuthStore)?
        };

        // 1. Prepare auth group "add" control message.

        let (auth_y, auth_message) = {
            let auth_control_message = AuthControlMessage {
                group_id: self.id(),
                action: AuthGroupAction::Add { member, access },
            };
            AuthGroup::prepare(auth_y, &auth_control_message).map_err(SpaceError::AuthGroup)?
        };

        // 2. Prepare and process encryption control message(s).

        let (encryption_y, encryption_message) = Self::process_group_membership_change(
            self.manager.clone(),
            self.id(),
            y.encryption_y,
            &auth_message,
        )
        .await?;
        y.encryption_y = encryption_y;

        // 3. Merge and sign control messages in forge (F).

        let message = Self::forge(
            self.manager.clone(),
            self.id(),
            auth_message,
            encryption_message,
        )
        .await?;

        // 4. Process auth control message.

        let mut auth_y = {
            let auth_message = AuthMessage::from_forged(&message);
            AuthGroup::process(auth_y, &auth_message).map_err(SpaceError::AuthGroup)?
        };

        // 5. Update auth and encryption orderer states.

        (y.encryption_y.orderer, auth_y.orderer_y) =
            Self::update_orderer_states(y.encryption_y.orderer, auth_y.orderer_y, &message);

        // 6. Persist new state.

        Self::set_state(self.manager.clone(), y, auth_y).await?;

        Ok(message)
    }

    pub(crate) async fn process(
        &self,
        message: &M,
    ) -> Result<Vec<Event>, SpaceError<S, F, M, C, RS>> {
        let events = match message.args() {
            SpacesArgs::KeyBundle {} => unreachable!("can't process key bundles here"),
            SpacesArgs::ControlMessage { id, .. } => {
                assert_eq!(id, &self.id); // Sanity check.
                self.process_control_message(message).await?
            }
            SpacesArgs::Application { space_id, .. } => {
                assert_eq!(space_id, &self.id); // Sanity check.
                self.process_application_message(message).await?
            }
        };

        Ok(events)
    }

    async fn process_control_message(
        &self,
        message: &M,
    ) -> Result<Vec<Event>, SpaceError<S, F, M, C, RS>> {
        let mut y = Self::get_or_init_state(self.id, self.manager.clone()).await?;

        // 1. Process auth control message.

        let mut auth_y = {
            let manager = self.manager.inner.read().await;
            let auth_message = AuthMessage::from_forged(message);
            let auth_y = manager.store.auth().await.map_err(SpaceError::AuthStore)?;
            AuthGroup::process(auth_y, &auth_message).map_err(SpaceError::AuthGroup)?
        };

        // 2. Process encryption control message.

        let (encryption_y, _encryption_output) = {
            let encryption_message = EncryptionMessage::from_forged(message);
            if let Some(encryption_message) = encryption_message {
                // Make encryption DGM aware of current auth members state.
                let group_members = auth_y.members(self.id);
                let secret_members = secret_members(group_members);
                y.encryption_y.dcgka.dgm = EncryptionMembershipState {
                    members: HashSet::from_iter(secret_members.into_iter()),
                };

                EncryptionGroup::receive(y.encryption_y, &encryption_message)
                    .map_err(SpaceError::EncryptionGroup)?
            } else {
                (y.encryption_y, vec![])
            }
        };
        y.encryption_y = encryption_y;

        // 3. Update auth and encryption orderer states.

        (y.encryption_y.orderer, auth_y.orderer_y) =
            Self::update_orderer_states(y.encryption_y.orderer, auth_y.orderer_y, message);

        // 4. Persist new state.

        Self::set_state(self.manager.clone(), y, auth_y).await?;

        Ok(vec![])
    }

    /// Process a group membership change on the group encryption state.
    ///
    /// The difference between the current and next secret group members (those with "read"
    /// access) is computed and only the diff processed in the encryption group.
    async fn process_group_membership_change(
        manager_ref: Manager<S, F, M, C, RS>,
        space_id: ActorId,
        mut encryption_y: EncryptionGroupState<M>,
        auth_message: &AuthMessage<C>,
    ) -> Result<(EncryptionGroupState<M>, Option<EncryptionMessage>), SpaceError<S, F, M, C, RS>>
    {
        // Compute the secret group members after this auth action will be processed, This is
        // needed to know which members have been added/removed from the encryption scope (those
        // that have read access) once the action has been processed.
        let (current_members, next_members) =
            Self::compute_secret_members(manager_ref.clone(), space_id, auth_message).await?;

        // Make the DGM aware of current members.
        let dgm = EncryptionMembershipState {
            members: HashSet::from_iter(current_members.clone().into_iter()),
        };
        encryption_y.dcgka.dgm = dgm;

        let (encryption_y, message) = {
            let manager = manager_ref.inner.read().await;
            match &auth_message.payload().action {
                AuthGroupAction::Create { .. } => {
                    EncryptionGroup::create(encryption_y, next_members.clone(), &manager.rng)
                        .map_err(SpaceError::EncryptionGroup)
                }
                AuthGroupAction::Add { .. } => {
                    let added = added_members(current_members, next_members);

                    if added.is_empty() {
                        return Ok((encryption_y, None));
                    }

                    // @TODO: here we just take the first added member, but actually we want to add
                    // every new member. For this we need to attach an array of encryption messages to
                    // the space control message.
                    EncryptionGroup::add(encryption_y, *added.first().unwrap(), &manager.rng)
                        .map_err(SpaceError::EncryptionGroup)
                }
                _ => unimplemented!(),
            }?
        };

        Ok((encryption_y, Some(message)))
    }

    async fn process_application_message(
        &self,
        message: &M,
    ) -> Result<Vec<Event>, SpaceError<S, F, M, C, RS>> {
        let mut y = self.state().await?;

        // Process encryption message.

        let (encryption_y, encryption_output) = {
            let encryption_message =
                EncryptionMessage::from_forged(message).expect("has encryption message");

            // @TODO: This calls members in the DGM
            EncryptionGroup::receive(y.encryption_y, &encryption_message)
                .map_err(SpaceError::EncryptionGroup)?
        };
        y.encryption_y = encryption_y;

        // Persist new state.

        let mut manager = self.manager.inner.write().await;
        manager
            .store
            .set_space(&self.id, y)
            .await
            .map_err(SpaceError::SpaceStore)?;

        let events = encryption_output
            .into_iter()
            .map(|event| {
                match event {
                    EncryptionGroupOutput::Application { plaintext } => Event::Application {
                        space_id: self.id,
                        data: plaintext,
                    },
                    _ => {
                        // We only expect "application" events inside this function.
                        unreachable!();
                    }
                }
            })
            .collect();

        Ok(events)
    }

    /// Process an auth control message before the "authored" version has been forged. This is
    /// useful when we want to know what the resulting auth state will be.
    async fn compute_secret_members(
        manager_ref: Manager<S, F, M, C, RS>,
        space_id: ActorId,
        auth_message: &AuthMessage<C>,
    ) -> Result<(Vec<ActorId>, Vec<ActorId>), SpaceError<S, F, M, C, RS>> {
        let manager = manager_ref.inner.read().await;
        let my_id = manager.forge.public_key().into();
        let auth_y = manager.store.auth().await.map_err(SpaceError::AuthStore)?;
        let current_members = secret_members(auth_y.members(space_id));

        // We process a fake operation on the current auth state in order to compute the next
        // membership state.
        let fake_op = AuthMessage::Forged {
            author: my_id,
            operation_id: OperationId::placeholder(),
            args: AuthArgs {
                dependencies: auth_message.dependencies(),
                control_message: auth_message.payload(),
            },
        };

        // NOTE: as we are only calling this method when we ourselves want to make space
        // membership changes locally, no operations will be concurrent to our local auth
        // state, and so no re-build will occur. This means processing the operation is cheap.
        // Processing the operation through the auth api is preferred over calculating the
        // next state manually, as auth does as some basic validation of control messages for
        // us and handles nested groups.
        let auth_y_i = AuthGroup::process(auth_y, &fake_op).map_err(SpaceError::AuthGroup)?;
        let next_members = secret_members(auth_y_i.members(space_id));
        Ok((current_members, next_members))
    }

    /// Update states for both encryption and auth orderers based on newly forged message.
    fn update_orderer_states(
        mut encryption_y: EncryptionOrdererState,
        mut auth_y: AuthOrdererState,
        message: &M,
    ) -> (EncryptionOrdererState, AuthOrdererState) {
        let encryption_message = EncryptionMessage::from_forged(message);
        match message.args() {
            SpacesArgs::KeyBundle {} => unimplemented!(),
            SpacesArgs::ControlMessage {
                auth_dependencies,
                encryption_dependencies,
                ..
            } => {
                auth_y.add_dependency(message.id(), auth_dependencies);

                if encryption_message.is_some() {
                    encryption_y.add_dependency(message.id(), encryption_dependencies);
                }
            }
            // @TODO: also include application messages in auth and encryption dependencies.
            SpacesArgs::Application { .. } => (),
        };
        (encryption_y, auth_y)
    }

    /// Forge a space message from an auth and optional encryption message. This produces a signed
    /// message which can be hashed to compute the final operation id.
    async fn forge(
        manager_ref: Manager<S, F, M, C, RS>,
        space_id: ActorId,
        auth_message: AuthMessage<C>,
        encryption_message: Option<EncryptionMessage>,
    ) -> Result<M, SpaceError<S, F, M, C, RS>> {
        let AuthMessage::Args(auth_args) = auth_message else {
            panic!("here we're only dealing with local operations");
        };

        let encryption_args =
            if let Some(EncryptionMessage::Args(encryption_args)) = encryption_message {
                Some(encryption_args)
            } else {
                None
            };

        let args = SpacesArgs::from_args(space_id, Some(auth_args), encryption_args);

        // @TODO: Can't use ephemeral private key for signing "create" message as this will make
        // the author / sender different from the person we want to do a key agreement with when
        // processing it in `p2panda-encryption`.
        let mut manager = manager_ref.inner.write().await;
        let message = manager.forge.forge(args).await.map_err(SpaceError::Forge)?;
        Ok(message)
    }

    /// Get the space state.
    async fn state(&self) -> Result<SpaceState<M>, SpaceError<S, F, M, C, RS>> {
        let manager = self.manager.inner.read().await;
        let space_y = manager
            .store
            .space(&self.id)
            .await
            .map_err(SpaceError::SpaceStore)?
            .ok_or(SpaceError::UnknownSpace(self.id))?;
        Ok(space_y)
    }

    /// Persist both auth and space state.
    async fn set_state(
        manager_ref: Manager<S, F, M, C, RS>,
        space: SpaceState<M>,
        auth: AuthGroupState<C>,
    ) -> Result<(), SpaceError<S, F, M, C, RS>> {
        let mut manager = manager_ref.inner.write().await;
        manager
            .store
            .set_auth(&auth)
            .await
            .map_err(SpaceError::AuthStore)?;
        let space_id = space.space_id;
        manager
            .store
            .set_space(&space_id, space)
            .await
            .map_err(SpaceError::SpaceStore)?;

        Ok(())
    }

    /// Get or if not present initialize a new space state.
    async fn get_or_init_state(
        space_id: ActorId,
        manager_ref: Manager<S, F, M, C, RS>,
    ) -> Result<SpaceState<M>, SpaceError<S, F, M, C, RS>> {
        let manager = manager_ref.inner.read().await;

        let result = manager
            .store
            .space(&space_id)
            .await
            .map_err(SpaceError::SpaceStore)?;

        let space_y = match result {
            Some(y) => y,
            None => {
                let my_id: ActorId = manager.forge.public_key().into();

                let key_manager_y = manager
                    .store
                    .key_manager()
                    .await
                    .map_err(SpaceError::KeyStore)?;

                let key_registry_y = manager
                    .store
                    .key_registry()
                    .await
                    .map_err(SpaceError::KeyStore)?;

                let dgm = EncryptionMembershipState {
                    members: HashSet::new(),
                };

                // Encryption orderer state is empty when we're initializing a new encryption
                // state.
                let orderer_y = EncryptionOrdererState::new();

                let encryption_y =
                    EncryptionGroup::init(my_id, key_manager_y, key_registry_y, dgm, orderer_y);
                SpaceState::from_state(space_id, encryption_y)
            }
        };
        Ok(space_y)
    }

    pub fn id(&self) -> ActorId {
        self.id
    }

    pub async fn members(&self) -> Result<Vec<(ActorId, Access<C>)>, SpaceError<S, F, M, C, RS>> {
        let manager = self.manager.inner.read().await;
        let auth_y = manager.store.auth().await.map_err(SpaceError::AuthStore)?;
        let group_members = auth_y.members(self.id);
        Ok(group_members)
    }

    pub async fn publish(&self, plaintext: &[u8]) -> Result<M, SpaceError<S, F, M, C, RS>> {
        let mut space_y = self.state().await?;
        let mut manager = self.manager.inner.write().await;

        let (encryption_y, encryption_args) =
            EncryptionGroup::send(space_y.encryption_y, plaintext, &manager.rng)
                .map_err(SpaceError::EncryptionGroup)?;

        let args = {
            let EncryptionMessage::Args(encryption_args) = encryption_args else {
                panic!("here we're only dealing with local operations");
            };

            SpacesArgs::from_args(self.id, None, Some(encryption_args))
        };

        space_y.encryption_y = encryption_y;

        manager
            .store
            .set_space(&self.id, space_y)
            .await
            .map_err(SpaceError::SpaceStore)?;

        let message = manager.forge.forge(args).await.map_err(SpaceError::Forge)?;

        Ok(message)
    }
}

#[derive(Debug)]
#[cfg_attr(any(test, feature = "test_utils"), derive(Clone))]
pub struct SpaceState<M> {
    pub space_id: ActorId,
    // @TODO: This contains the PKI and KMG states and other unnecessary data we don't need to
    // persist. We can make the fields public in `p2panda-encryption` and extract only the
    // information we really need.
    pub encryption_y: EncryptionGroupState<M>,
}

impl<M> SpaceState<M> {
    pub fn from_state(space_id: ActorId, encryption_y: EncryptionGroupState<M>) -> Self {
        Self {
            space_id,
            encryption_y,
        }
    }
}

pub fn secret_members<C>(members: Vec<(ActorId, Access<C>)>) -> Vec<ActorId> {
    members
        .into_iter()
        .filter_map(|(id, access)| if access.is_pull() { None } else { Some(id) })
        .collect()
}

pub fn added_members(current_members: Vec<ActorId>, next_members: Vec<ActorId>) -> Vec<ActorId> {
    next_members
        .iter()
        .cloned()
        .filter(|actor| !current_members.contains(actor))
        .collect::<Vec<_>>()
}

#[derive(Debug, Error)]
pub enum SpaceError<S, F, M, C, RS>
where
    S: SpaceStore<M> + KeyStore + AuthStore<C>,
    F: Forge<M, C>,
    C: Conditions,
    RS: AuthResolver<C>,
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
    KeyStore(<S as KeyStore>::Error),

    #[error("{0}")]
    SpaceStore(<S as SpaceStore<M>>::Error),

    #[error("tried to access unknown space id {0}")]
    UnknownSpace(ActorId),
}
