// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fmt::Debug;

use p2panda_auth::Access;
use p2panda_auth::group::{GroupAction as AuthGroupAction, GroupCrdt as AuthGroup, GroupMember};
use p2panda_auth::traits::Resolver;
use p2panda_core::PrivateKey;
use p2panda_encryption::data_scheme::EncryptionGroup;
use thiserror::Error;

use crate::manager::Manager;
use crate::orderer::AuthOrderer;
use crate::traits::Forge;
use crate::{
    ActorId, AuthControlMessage, AuthDummyStore, AuthGroupError, AuthGroupState, Conditions,
    OperationId,
};

/// Encrypted data context with authorization boundary.
///
/// Only members with suitable access to the space can read and write to it.
pub struct Space<S, F, M, C, RS> {
    manager: Manager<S, F, M, C, RS>,
}

impl<S, F, M, C, RS> Space<S, F, M, C, RS>
where
    C: Conditions,
    F: Forge<M>,
    RS: Debug + Resolver<ActorId, OperationId, C, AuthOrderer, AuthDummyStore>,
{
    pub(crate) async fn create(
        manager_ref: Manager<S, F, M, C, RS>,
        mut initial_members: Vec<(GroupMember<ActorId>, Access<C>)>,
    ) -> Result<Self, SpaceError<C, RS>> {
        let manager = manager_ref.inner.read().await;

        let my_id: ActorId = manager.forge.public_key().into();

        // 1. Derive a space id.

        // @TODO
        //    - generate new key pair
        //    - use public key for space id
        //    - use the private key to sign the control message
        //    - throw away the private key
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

        // let encryption_y = EncryptionGroup::init(my_id, my_keys, pki, dgm, orderer);

        // 3. Prepare encryption group state.

        // 3. establish encryption group state with create control message
        // 4. merge and sign control messages in forge (F)
        // 5. persist new state

        drop(manager);

        Ok(Self {
            manager: manager_ref,
        })
    }

    pub fn publish(_bytes: &[u8]) {
        todo!()
    }

    pub fn process(&mut self, _message: &M) {
        todo!()
    }
}

#[derive(Debug, Error)]
pub enum SpaceError<C, RS>
where
    RS: Resolver<ActorId, OperationId, C, AuthOrderer, AuthDummyStore>,
{
    #[error("{0}")]
    AuthGroup(AuthGroupError<C, RS>),
}
