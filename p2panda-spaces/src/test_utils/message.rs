// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_core::{Hash, PublicKey};

use crate::message::SpacesArgs;
use crate::test_utils::{TestConditions, TestSpaceId};
use crate::traits::{AuthoredMessage, SpacesMessage};
use crate::{ActorId, OperationId};

pub type SeqNum = u64;

#[derive(Clone, Debug)]
pub struct TestMessage {
    pub seq_num: SeqNum,
    pub public_key: PublicKey,
    pub spaces_args: SpacesArgs<TestSpaceId, TestConditions>,
}

impl AuthoredMessage for TestMessage {
    fn id(&self) -> OperationId {
        let mut buffer: Vec<u8> = self.public_key.as_bytes().to_vec();
        buffer.extend_from_slice(&self.seq_num.to_be_bytes());
        Hash::new(buffer).into()
    }

    fn author(&self) -> ActorId {
        self.public_key.into()
    }
}

impl SpacesMessage<TestSpaceId, TestConditions> for TestMessage {
    fn args(&self) -> &SpacesArgs<TestSpaceId, TestConditions> {
        &self.spaces_args
    }
}
