// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fmt::Debug;

use p2panda_auth::Access;
use p2panda_auth::group::{GroupAction as AuthGroupAction, GroupCrdt as AuthGroup, GroupMember};
use p2panda_auth::traits::Resolver;
use p2panda_core::PrivateKey;
use thiserror::Error;

use crate::auth::orderer::{AuthMessage, AuthOrderer};
use crate::encryption::dgm::EncryptionMembershipState;
use crate::encryption::orderer::EncryptionMessage;
use crate::forge::{Forge, ForgeArgs, ForgedMessage};
use crate::manager::Manager;
use crate::types::{
    ActorId, AuthControlMessage, AuthDummyStore, AuthGroupError, AuthGroupState, Conditions,
    EncryptionGroup, EncryptionGroupError, OperationId,
};

/// Encrypted data context with authorization boundary.
///
/// Only members with suitable access to the space can read and write to it.
#[derive(Debug)]
pub struct Space<S, F, M, C, RS> {
    manager: Manager<S, F, M, C, RS>,
}

impl<S, F, M, C, RS> Space<S, F, M, C, RS>
where
    F: Forge<M, C>,
    M: ForgedMessage<C>,
    C: Conditions,
    RS: Debug + Resolver<ActorId, OperationId, C, AuthOrderer, AuthDummyStore>,
{
    #[allow(clippy::result_large_err)]
    pub(crate) fn create(
        manager_ref: Manager<S, F, M, C, RS>,
        mut initial_members: Vec<(GroupMember<ActorId>, Access<C>)>,
    ) -> Result<Self, SpaceError<F, M, C, RS>> {
        let manager = manager_ref.inner.borrow_mut();

        let my_id: ActorId = manager.forge.public_key().into();

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
            // @TODO: Get this from manager & establish initial orderer state.
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

        // 3. Establish encryption group state with create control message.

        let (_encryption_y, encryption_args) = {
            let dgm = EncryptionMembershipState {
                space_id,
                group_store: (),
            };

            let orderer_y = ();

            // @TODO: KeyManagerState and KeyRegistryState should be shared across groups and so we
            // need a wrapper around them which follows interior mutability patterns.
            let y = EncryptionGroup::init(
                my_id,
                manager.key_manager_y.clone(), // @TODO: Get state from S instead
                manager.key_registry_y.clone(), // @TODO: Get state from S instead
                dgm,
                orderer_y,
            );

            let members = auth_y.transitive_members().map_err(SpaceError::AuthGroup)?;
            let secret_members = secret_members(members);
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

        let message = manager
            .forge
            .forge_ephemeral(ephemeral_private_key, args)
            .map_err(SpaceError::Forge)?;

        // 5. Process auth message.

        let _auth_y = {
            let auth_message = AuthMessage::from_forged(message);
            AuthGroup::process(auth_y, &auth_message).map_err(SpaceError::AuthGroup)?
        };

        // 6. Persist new state.

        // @TODO: Write new state to S (Store).

        drop(manager);

        Ok(Self {
            manager: manager_ref,
        })
    }

    pub(crate) fn process(&mut self, _message: &M) {
        todo!()
    }

    pub fn publish(_bytes: &[u8]) {
        todo!()
    }
}

fn secret_members<C>(members: Vec<(ActorId, Access<C>)>) -> Vec<ActorId> {
    members
        .into_iter()
        .filter_map(|(id, access)| if access.is_pull() { None } else { Some(id) })
        .collect()
}

#[derive(Debug, Error)]
pub enum SpaceError<F, M, C, RS>
where
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
}
