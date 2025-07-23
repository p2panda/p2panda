// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fmt::Debug;

use p2panda_core::{PrivateKey, PublicKey};

use crate::auth::orderer::AuthArgs;
use crate::encryption::orderer::EncryptionArgs;
use crate::message::ControlMessage;
use crate::types::{
    ActorId, AuthGroupAction, Conditions, EncryptionControlMessage, EncryptionDirectMessage,
    OperationId,
};

pub trait Forge<M, C>
where
    M: ForgedMessage<C>,
{
    type Error: Debug;

    fn public_key(&self) -> PublicKey;

    fn forge(&mut self, args: ForgeArgs<C>) -> impl Future<Output = Result<M, Self::Error>>;

    fn forge_ephemeral(
        &mut self,
        private_key: PrivateKey,
        args: ForgeArgs<C>,
    ) -> impl Future<Output = Result<M, Self::Error>>;
}

pub trait ForgedMessage<C> {
    fn id(&self) -> OperationId;

    fn author(&self) -> ActorId;

    fn group_id(&self) -> ActorId;

    fn control_message(&self) -> &ControlMessage<C>;
}

#[derive(Debug)]
#[cfg_attr(any(test, feature = "test_utils"), derive(Clone))]
pub struct ForgeArgs<C> {
    pub group_id: ActorId,
    pub control_message: ControlMessage<C>,
    pub direct_messages: Vec<EncryptionDirectMessage>,
}

impl<C> ForgeArgs<C>
where
    C: Conditions,
{
    pub(crate) fn from_args(
        group_id: ActorId,
        auth_args: Option<AuthArgs<C>>,
        encryption_args: Option<EncryptionArgs>,
    ) -> Self {
        let auth_action = auth_args.map(|args| args.control_message.action);
        let (encryption_action, direct_messages) = {
            match encryption_args {
                Some(args) => (Some(args.control_message), args.direct_messages),
                None => (None, Vec::new()),
            }
        };

        let control_message = {
            let (auth_action, encryption_action) = match (auth_action, encryption_action) {
                (None, Some(_)) => todo!(),
                (Some(_), None) => todo!(),
                (Some(auth_action), Some(encryption_action)) => (auth_action, encryption_action),
                _ => {
                    panic!("invalid arguments")
                }
            };

            match (auth_action, encryption_action) {
                (
                    AuthGroupAction::Create { initial_members },
                    EncryptionControlMessage::Create { .. },
                ) => ControlMessage::Create { initial_members },
                _ => unimplemented!(),
            }

            // @TODO
            // (AuthGroupAction::Create { initial_members }, EncryptionControlMessage::Update) => todo!(),
            // (
            //     AuthGroupAction::Create { initial_members },
            //     EncryptionControlMessage::Remove { removed },
            // ) => todo!(),
            // (AuthGroupAction::Create { initial_members }, EncryptionControlMessage::Add { added }) => {
            //     todo!()
            // }
            // (
            //     AuthGroupAction::Add { member, access },
            //     EncryptionControlMessage::Create { initial_members },
            // ) => todo!(),
            // (AuthGroupAction::Add { member, access }, EncryptionControlMessage::Update) => todo!(),
            // (AuthGroupAction::Add { member, access }, EncryptionControlMessage::Remove { removed }) => {
            //     todo!()
            // }
            // (AuthGroupAction::Add { member, access }, EncryptionControlMessage::Add { added }) => {
            //     todo!()
            // }
            // (
            //     AuthGroupAction::Remove { member },
            //     EncryptionControlMessage::Create { initial_members },
            // ) => todo!(),
            // (AuthGroupAction::Remove { member }, EncryptionControlMessage::Update) => todo!(),
            // (AuthGroupAction::Remove { member }, EncryptionControlMessage::Remove { removed }) => {
            //     todo!()
            // }
            // (AuthGroupAction::Remove { member }, EncryptionControlMessage::Add { added }) => todo!(),
            // (
            //     AuthGroupAction::Promote { member, access },
            //     EncryptionControlMessage::Create { initial_members },
            // ) => todo!(),
            // (AuthGroupAction::Promote { member, access }, EncryptionControlMessage::Update) => todo!(),
            // (
            //     AuthGroupAction::Promote { member, access },
            //     EncryptionControlMessage::Remove { removed },
            // ) => todo!(),
            // (AuthGroupAction::Promote { member, access }, EncryptionControlMessage::Add { added }) => {
            //     todo!()
            // }
            // (
            //     AuthGroupAction::Demote { member, access },
            //     EncryptionControlMessage::Create { initial_members },
            // ) => todo!(),
            // (AuthGroupAction::Demote { member, access }, EncryptionControlMessage::Update) => todo!(),
            // (
            //     AuthGroupAction::Demote { member, access },
            //     EncryptionControlMessage::Remove { removed },
            // ) => todo!(),
            // (AuthGroupAction::Demote { member, access }, EncryptionControlMessage::Add { added }) => {
            //     todo!()
            // }
        };

        Self {
            group_id,
            control_message,
            direct_messages,
        }
    }
}
