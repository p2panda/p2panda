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

        // 1. Derive a space id.

        let ephemeral_private_key = {
            let manager = manager_ref.inner.write().await;
            PrivateKey::from_bytes(&manager.rng.random_array()?)
        };
        let space_id: ActorId = ephemeral_private_key.public_key().into();

        // 2a. Prepare auth group state with "create" control message.

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

        let (auth_y, auth_args) = {
            let manager = manager_ref.inner.read().await;
            let auth_y = manager.store.auth().await.map_err(SpaceError::AuthStore)?;
            AuthGroup::prepare(auth_y, &auth_control_message).map_err(SpaceError::AuthGroup)?
        };

        // 2b. Compute the new group state to get the secret members.

        let (secret_members, _) = Self::pre_process_auth(
            manager_ref.clone(),
            space_id,
            &auth_args.dependencies(),
            &auth_control_message,
        )
        .await?;

        // 3. Establish encryption group state (prepare & process) with "create" control message.

        let (encryption_y, encryption_args) = {
            let manager = manager_ref.inner.read().await;

            // Establish DGM state.

            let dgm = EncryptionMembershipState {
                members: HashSet::from_iter(secret_members.iter().cloned()),
            };

            // @TODO: Get this from store & establish initial orderer state.
            //
            // This initial orderer state is not necessarily "empty", can include pointers at other
            // groups in case we've passed in "groups" as our initial members.
            let orderer_y = EncryptionOrdererState::new();

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

            let y = EncryptionGroup::init(my_id, key_manager_y, key_registry_y, dgm, orderer_y);

            // We can use the high-level API (prepare & process internally) as none of the internal
            // methods require a signed message type ("forged") with a final "operation id".
            EncryptionGroup::create(y, secret_members, &manager.rng)
                .map_err(SpaceError::EncryptionGroup)?
        };

        // 4. Merge and sign control messages in forge (F).

        let args = {
            let AuthMessage::Args(auth_args) = auth_args else {
                panic!("here we're only dealing with local operations");
            };

            let EncryptionMessage::Args(encryption_args) = encryption_args else {
                panic!("here we're only dealing with local operations");
            };

            SpacesArgs::from_args(space_id, Some(auth_args), Some(encryption_args))
        };

        let mut manager = manager_ref.inner.write().await;

        // @TODO: Can't use ephemeral private key for signing "create" message as this will make
        // the author / sender different from the person we want to do a key agreement with when
        // processing it in `p2panda-encryption`.
        let message = manager.forge.forge(args).await.map_err(SpaceError::Forge)?;
        drop(manager);

        // 5. Process auth message.

        let auth_y = {
            let auth_message = AuthMessage::from_forged(&message);
            AuthGroup::process(auth_y, &auth_message).map_err(SpaceError::AuthGroup)?
        };

        // 6. Persist new state.

        let y = SpaceState::from_state(space_id, encryption_y);
        Self::persist_space(manager_ref.clone(), y, auth_y).await?;

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
        let mut y = {
            let manager = self.manager.inner.read().await;
            manager
                .store
                .space(&self.id)
                .await
                .map_err(SpaceError::SpaceStore)?
                .expect("space exists")
        };

        // 1. Prepare auth group state with "add" control message.

        let auth_control_message = AuthControlMessage {
            group_id: self.id(),
            action: AuthGroupAction::Add { member, access },
        };

        let (auth_y, auth_args) = {
            let manager = self.manager.inner.read().await;
            let auth_y = manager.store.auth().await.map_err(SpaceError::AuthStore)?;
            AuthGroup::prepare(auth_y, &auth_control_message).map_err(SpaceError::AuthGroup)?
        };

        // 2. Compute the secret group members _after_ this operation will be processed, and the
        //    diff between the _current_ and _next_ secret members. This is needed to know which
        //    members have been added to the encryption scope (they have read access) once the
        //    "add" has been processed.

        let (secret_members, _secret_members_diff) = Self::pre_process_auth(
            self.manager.clone(),
            self.id(),
            &auth_args.dependencies(),
            &auth_control_message,
        )
        .await?;

        // 3. Add new secret members to encryption group state.

        let (encryption_y, encryption_args) = {
            let mut manager = self.manager.inner.write().await;

            // Make encryption DGM aware of current auth members state.

            y.encryption_y.dcgka.dgm = EncryptionMembershipState {
                members: HashSet::from_iter(secret_members.into_iter()),
            };

            // Share "global" key-manager and -registry across all spaces.

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

            y.encryption_y.dcgka.my_keys = key_manager_y;
            y.encryption_y.dcgka.pki = key_registry_y;

            match member {
                GroupMember::Individual(id) => {
                    EncryptionGroup::add(y.encryption_y, id, &manager.rng)
                        .map_err(SpaceError::EncryptionGroup)?
                }
                GroupMember::Group(_) => {
                    // @TODO: in order to support adding a group to a space we need to add _all_
                    // new transitive members which have read access. We already compute the new
                    // to-be-added secret members above and have it in _secret_members_diff.
                    unimplemented!();
                }
            }
        };
        y.encryption_y = encryption_y;

        // 4. Merge and sign control messages in forge (F).

        let args = {
            let AuthMessage::Args(auth_args) = auth_args else {
                panic!("here we're only dealing with local operations");
            };

            let EncryptionMessage::Args(encryption_args) = encryption_args else {
                panic!("here we're only dealing with local operations");
            };

            SpacesArgs::from_args(self.id, Some(auth_args), Some(encryption_args))
        };

        let mut manager = self.manager.inner.write().await;

        // @TODO: Can't use ephemeral private key for signing "create" message as this will make
        // the author / sender different from the person we want to do a key agreement with when
        // processing it in `p2panda-encryption`.
        let message = manager.forge.forge(args).await.map_err(SpaceError::Forge)?;
        drop(manager);

        // 5. Process auth message.

        let auth_y = {
            let auth_message = AuthMessage::from_forged(&message);
            AuthGroup::process(auth_y, &auth_message).map_err(SpaceError::AuthGroup)?
        };

        // 6. Persist new state.

        Self::persist_space(self.manager.clone(), y, auth_y).await?;

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
        let mut y = {
            let manager = self.manager.inner.read().await;

            let my_id: ActorId = manager.forge.public_key().into();

            match manager
                .store
                .space(&self.id)
                .await
                .map_err(SpaceError::SpaceStore)?
            {
                Some(y) => y,
                None => {
                    // @TODO: This repeats quite a lot, would be good to factor state
                    // initialisation out.
                    let encryption_y = {
                        // Establish DGM state.
                        let dgm = EncryptionMembershipState {
                            members: HashSet::new(),
                        };

                        // @TODO: Get this from store & establish initial orderer state.
                        //
                        // This initial orderer state is not necessarily "empty", can include pointers at other
                        // groups in case we've passed in "groups" as our initial members.
                        let orderer_y = EncryptionOrdererState::new();

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

                        EncryptionGroup::init(my_id, key_manager_y, key_registry_y, dgm, orderer_y)
                    };

                    SpaceState::from_state(self.id, encryption_y)
                }
            }
        };

        // Process auth message.

        let auth_y = {
            let manager = self.manager.inner.read().await;
            let auth_message = AuthMessage::from_forged(message);
            let auth_y = manager.store.auth().await.map_err(SpaceError::AuthStore)?;
            AuthGroup::process(auth_y, &auth_message).map_err(SpaceError::AuthGroup)?
        };

        // Process encryption message.

        let (encryption_y, _encryption_output) = {
            let manager = self.manager.inner.read().await;

            // Make encryption DGM aware of current auth members state.

            let group_members = auth_y.members(self.id);
            let secret_members = secret_members(group_members);

            y.encryption_y.dcgka.dgm = EncryptionMembershipState {
                members: HashSet::from_iter(secret_members.into_iter()),
            };

            // Share "global" key-manager and -registry across all spaces.

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

            y.encryption_y.dcgka.my_keys = key_manager_y;
            y.encryption_y.dcgka.pki = key_registry_y;

            let encryption_message = EncryptionMessage::from_forged(message);

            EncryptionGroup::receive(y.encryption_y, &encryption_message)
                .map_err(SpaceError::EncryptionGroup)?
        };
        y.encryption_y = encryption_y;

        // Persist new state.

        Self::persist_space(self.manager.clone(), y, auth_y).await?;

        Ok(vec![])
    }

    async fn process_application_message(
        &self,
        message: &M,
    ) -> Result<Vec<Event>, SpaceError<S, F, M, C, RS>> {
        let mut y = self.state().await?;

        // Process encryption message.

        let (encryption_y, encryption_output) = {
            let encryption_message = EncryptionMessage::from_forged(message);

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
    async fn pre_process_auth(
        manager: Manager<S, F, M, C, RS>,
        space_id: ActorId,
        dependencies: &Vec<OperationId>,
        control_message: &AuthControlMessage<C>,
    ) -> Result<(Vec<ActorId>, Vec<ActorId>), SpaceError<S, F, M, C, RS>> {
        let manager = manager.inner.read().await;
        let my_id: ActorId = manager.forge.public_key().into();
        let auth_y = manager.store.auth().await.map_err(SpaceError::AuthStore)?;
        let previous_members = secret_members(auth_y.members(space_id));

        // We process a fake operation on the current auth state in order to compute the next
        // membership state.
        let fake_op = AuthMessage::Forged {
            author: my_id,
            operation_id: OperationId::placeholder(),
            args: AuthArgs {
                dependencies: dependencies.to_owned(),
                control_message: control_message.to_owned(),
            },
        };

        // NOTE: as we are only calling this method when we ourselves want to make space
        // membership changes locally, no operations will be concurrent to our local auth
        // state, and so no re-build will occur. This means processing the operation is cheap.
        // Processing the operation through the auth api is preferred over calculating the
        // next state manually, as auth does as some basic validation of control messages for
        // us.
        let auth_y_i = AuthGroup::process(auth_y, &fake_op).map_err(SpaceError::AuthGroup)?;
        let next_members = secret_members(auth_y_i.members(space_id));
        let diff_members = next_members
            .iter()
            .cloned()
            .filter(|actor| previous_members.contains(actor))
            .collect::<Vec<_>>();
        Ok((next_members, diff_members))
    }

    // Persist both auth and space state.
    async fn persist_space(
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
