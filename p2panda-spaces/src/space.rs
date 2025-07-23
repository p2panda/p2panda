// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fmt::Debug;

use p2panda_auth::Access;
use p2panda_auth::group::GroupMember;
use p2panda_auth::traits::Resolver;
use p2panda_core::PrivateKey;
use thiserror::Error;

use crate::auth::orderer::{AuthMessage, AuthOrderer};
use crate::encryption::dgm::EncryptionMembershipState;
use crate::encryption::orderer::EncryptionMessage;
use crate::forge::{Forge, ForgeArgs, ForgedMessage};
use crate::manager::Manager;
use crate::store::{KeyStore, SpaceStore};
use crate::types::{
    ActorId, AuthControlMessage, AuthDummyStore, AuthGroup, AuthGroupAction, AuthGroupError,
    AuthGroupState, Conditions, EncryptionGroup, EncryptionGroupError, EncryptionGroupState,
    OperationId,
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
    S: SpaceStore<M, C, RS> + KeyStore,
    F: Forge<M, C>,
    M: ForgedMessage<C>,
    C: Conditions,
    RS: Debug + Resolver<ActorId, OperationId, C, AuthOrderer, AuthDummyStore>,
{
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

        let ephemeral_private_key = PrivateKey::new();
        let space_id: ActorId = ephemeral_private_key.public_key().into();

        // 2. Prepare auth group state with "create" control message.

        // Automatically add ourselves with "manage" level without any conditions as default.
        if !initial_members
            .iter()
            .any(|(member, _)| member.id() == my_id)
        {
            initial_members.push((GroupMember::Individual(my_id), Access::manage()));
        }

        let (auth_y, auth_args) = {
            // @TODO: Get this from store & establish initial orderer state.
            //
            // This initial orderer state is not necessarily "empty", can include pointers at other
            // groups in case we've passed in "groups" as our initial members.
            let orderer_y = ();

            let y = AuthGroupState::<C, RS>::new(my_id, space_id, AuthDummyStore, orderer_y);

            let action = AuthControlMessage {
                group_id: space_id,
                action: AuthGroupAction::Create { initial_members },
            };

            AuthGroup::prepare(y, &action).map_err(SpaceError::AuthGroup)?
        };

        // 3. Establish encryption group state (prepare & process) with "create" control message.

        let (encryption_y, encryption_args) = {
            let manager = manager_ref.inner.read().await;

            // @TODO: Establish DGM state.
            //
            // This will mostly be a wrapper around the auth state, as this is where we will learn
            // about the current group members.
            //
            // We keep the "space_id" around so the object knows which group state to look up.
            let dgm = EncryptionMembershipState {
                space_id,
                group_store: (),
            };

            // @TODO: Establish orderer state.
            let orderer_y = ();

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

            let group_members = auth_y.transitive_members().map_err(SpaceError::AuthGroup)?;
            let secret_members = secret_members(group_members);

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

            ForgeArgs::from_args(space_id, Some(auth_args), Some(encryption_args))
        };

        let mut manager = manager_ref.inner.write().await;

        let message = manager
            .forge
            .forge_ephemeral(ephemeral_private_key, args)
            .await
            .map_err(SpaceError::Forge)?;

        // 5. Process auth message.

        let auth_y = {
            let auth_message = AuthMessage::from_forged(&message);
            AuthGroup::process(auth_y, &auth_message).map_err(SpaceError::AuthGroup)?
        };

        // 6. Persist new state.

        manager
            .store
            .set_space(space_id, SpaceState::new(space_id, auth_y, encryption_y))
            .await
            .map_err(SpaceError::SpaceStore)?;

        drop(manager);

        Ok((
            Self {
                id: space_id,
                manager: manager_ref,
            },
            message,
        ))
    }

    pub(crate) fn process(&mut self, _message: &M) {
        todo!()
    }

    pub fn id(&self) -> ActorId {
        self.id
    }

    pub fn publish(_bytes: &[u8]) {
        todo!()
    }
}

#[derive(Debug)]
pub struct SpaceState<M, C, RS>
where
    C: Conditions,
{
    pub space_id: ActorId,
    pub auth_y: AuthGroupState<C, RS>,
    pub encryption_y: EncryptionGroupState<M>,
}

impl<M, C, RS> SpaceState<M, C, RS>
where
    C: Conditions,
{
    pub fn new(
        space_id: ActorId,
        auth_y: AuthGroupState<C, RS>,
        encryption_y: EncryptionGroupState<M>,
    ) -> Self {
        Self {
            space_id,
            auth_y,
            encryption_y,
        }
    }
}

fn secret_members<C>(members: Vec<(ActorId, Access<C>)>) -> Vec<ActorId> {
    members
        .into_iter()
        .filter_map(|(id, access)| if access.is_pull() { None } else { Some(id) })
        .collect()
}

#[derive(Debug, Error)]
pub enum SpaceError<S, F, M, C, RS>
where
    S: SpaceStore<M, C, RS> + KeyStore,
    F: Forge<M, C>,
    M: ForgedMessage<C>,
    C: Conditions,
    RS: Resolver<ActorId, OperationId, C, AuthOrderer, AuthDummyStore>,
{
    #[error("{0}")]
    AuthGroup(AuthGroupError<C, RS>),

    #[error("{0}")]
    EncryptionGroup(EncryptionGroupError<M>),

    #[error("{0}")]
    Forge(F::Error),

    #[error("{0}")]
    KeyStore(<S as KeyStore>::Error),

    #[error("{0}")]
    SpaceStore(<S as SpaceStore<M, C, RS>>::Error),
}
