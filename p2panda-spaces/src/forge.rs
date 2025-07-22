use std::convert::Infallible;
use std::fmt::Debug;

use p2panda_auth::Access;
use p2panda_auth::group::GroupMember;
use p2panda_core::{PrivateKey, PublicKey};
use p2panda_encryption::data_scheme::DirectMessage;

use crate::dgm::EncryptionGroupMembership;
use crate::orderer::{AuthArgs, EncryptionArgs};
use crate::{
    ActorId, AuthAction, AuthControlMessage, Conditions, EncryptionControlMessage, OperationId,
};

pub trait Forge<M, C>
where
    M: SpacesMessage<C>,
{
    type Error: Debug;

    fn public_key(&self) -> PublicKey;

    fn forge(&self, args: ForgeArgs<C>) -> Result<M, Self::Error>;

    fn forge_with(&self, private_key: PrivateKey, args: ForgeArgs<C>) -> Result<M, Self::Error>;
}

pub trait SpacesMessage<C> {
    fn id(&self) -> OperationId;

    fn author(&self) -> ActorId;

    fn group_id(&self) -> ActorId;

    fn control_message(&self) -> &ControlMessage<C>;
}

#[derive(Debug)]
pub enum ControlMessage<C> {
    Create {
        // GroupMember is required for understanding if a public key / actor id is an individual or
        // a group in case we're adding something with only pull-access. In that case that actor
        // doesn't need to publish a key bundle and every receiver will not strictly be able to
        // verify if it's _really_ a group or individual.
        //
        // In any other case we always want to verify if the group member type is correct.
        initial_members: Vec<(GroupMember<ActorId>, Access<C>)>,
    },
    // @TODO: introduce all other variants.
}

impl<C> ControlMessage<C>
where
    C: Conditions,
{
    pub(crate) fn to_auth_action(&self) -> AuthAction<C> {
        match self {
            ControlMessage::Create { initial_members } => AuthAction::Create {
                initial_members: initial_members.to_owned(),
            },
        }
    }
}

#[derive(Debug)]
pub struct ForgeArgs<C> {
    pub group_id: ActorId,
    pub control_message: ControlMessage<C>,
    pub direct_messages: Vec<DirectMessage<ActorId, OperationId, EncryptionGroupMembership>>,
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
                (None, None) => panic!(),
                (None, Some(_)) => todo!(),
                (Some(_), None) => todo!(),
                (Some(auth_action), Some(encryption_action)) => (auth_action, encryption_action),
            };

            match (auth_action, encryption_action) {
                (
                    AuthAction::Create { initial_members },
                    EncryptionControlMessage::Create { .. },
                ) => ControlMessage::Create { initial_members },
                _ => unimplemented!(),
            }

            // @TODO
            // (AuthAction::Create { initial_members }, EncryptionControlMessage::Update) => todo!(),
            // (
            //     AuthAction::Create { initial_members },
            //     EncryptionControlMessage::Remove { removed },
            // ) => todo!(),
            // (AuthAction::Create { initial_members }, EncryptionControlMessage::Add { added }) => {
            //     todo!()
            // }
            // (
            //     AuthAction::Add { member, access },
            //     EncryptionControlMessage::Create { initial_members },
            // ) => todo!(),
            // (AuthAction::Add { member, access }, EncryptionControlMessage::Update) => todo!(),
            // (AuthAction::Add { member, access }, EncryptionControlMessage::Remove { removed }) => {
            //     todo!()
            // }
            // (AuthAction::Add { member, access }, EncryptionControlMessage::Add { added }) => {
            //     todo!()
            // }
            // (
            //     AuthAction::Remove { member },
            //     EncryptionControlMessage::Create { initial_members },
            // ) => todo!(),
            // (AuthAction::Remove { member }, EncryptionControlMessage::Update) => todo!(),
            // (AuthAction::Remove { member }, EncryptionControlMessage::Remove { removed }) => {
            //     todo!()
            // }
            // (AuthAction::Remove { member }, EncryptionControlMessage::Add { added }) => todo!(),
            // (
            //     AuthAction::Promote { member, access },
            //     EncryptionControlMessage::Create { initial_members },
            // ) => todo!(),
            // (AuthAction::Promote { member, access }, EncryptionControlMessage::Update) => todo!(),
            // (
            //     AuthAction::Promote { member, access },
            //     EncryptionControlMessage::Remove { removed },
            // ) => todo!(),
            // (AuthAction::Promote { member, access }, EncryptionControlMessage::Add { added }) => {
            //     todo!()
            // }
            // (
            //     AuthAction::Demote { member, access },
            //     EncryptionControlMessage::Create { initial_members },
            // ) => todo!(),
            // (AuthAction::Demote { member, access }, EncryptionControlMessage::Update) => todo!(),
            // (
            //     AuthAction::Demote { member, access },
            //     EncryptionControlMessage::Remove { removed },
            // ) => todo!(),
            // (AuthAction::Demote { member, access }, EncryptionControlMessage::Add { added }) => {
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
