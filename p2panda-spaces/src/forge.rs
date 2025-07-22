use std::convert::Infallible;

use p2panda_auth::Access;
use p2panda_core::PrivateKey;
use p2panda_encryption::data_scheme::DirectMessage;

use crate::dgm::EncryptionGroupMembership;
use crate::orderer::{AuthArgs, EncryptionArgs};
use crate::traits::Forge;
use crate::{ActorId, Conditions, OperationId};

pub enum ControlMessage<C> {
    // @TODO: fill in required parameters
    Create {
        initial_members: Vec<(ActorId, Access<C>)>,
    },
    // TODO: introduce all other variants
}

pub struct ManagerForgeArgs<C> {
    group_id: ActorId,
    control_message: ControlMessage<C>,
    direct_messages: Vec<DirectMessage<ActorId, OperationId, EncryptionGroupMembership>>,
}

impl<C> ManagerForgeArgs<C>
where
    C: Conditions,
{
    pub(crate) fn from_args(
        auth_args: Option<AuthArgs<C>>,
        encryption_args: Option<EncryptionArgs>,
    ) -> Self {
        let auth_action = auth_args.map(|args| args.control_message.action);
        let encryption_action = encryption_args.map(|args| args.control_message);

        todo!()
    }
}
