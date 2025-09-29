// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_core::{Hash, PublicKey};

use crate::message::SpacesArgs;
use crate::test_utils::{SeqNum, TestConditions};
use crate::traits::message::{AuthoredMessage, SpacesMessage};
use crate::traits::SpaceId;
use crate::{ActorId, OperationId};

#[derive(Clone, Debug)]
pub struct TestMessage<ID> {
    pub seq_num: SeqNum,
    pub public_key: PublicKey,
    pub spaces_args: SpacesArgs<ID, TestConditions>,
}

impl<ID> AuthoredMessage for TestMessage<ID>
where
    ID: SpaceId,
{
    fn id(&self) -> OperationId {
        let mut buffer: Vec<u8> = self.public_key.as_bytes().to_vec();
        buffer.extend_from_slice(&self.seq_num.to_be_bytes());
        Hash::new(buffer).into()
    }

    fn author(&self) -> ActorId {
        self.public_key.into()
    }
}

impl<ID> SpacesMessage<ID, TestConditions> for TestMessage<ID> {
    fn args(&self) -> &SpacesArgs<ID, TestConditions> {
        &self.spaces_args
    }
}
