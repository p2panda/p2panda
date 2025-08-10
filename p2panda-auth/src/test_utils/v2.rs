// SPDX-License-Identifier: MIT OR Apache-2.0

use std::convert::Infallible;

use crate::group::GroupControlMessage;
use crate::group::crdt::v2::{GroupCrdt, GroupCrdtError, GroupCrdtState};
use crate::group::resolver_v2::StrongRemove;
use crate::test_utils::TestOperation;
use crate::traits::{IdentityHandle, OperationId, Orderer};

pub type MemberId = char;
pub type MessageId = u32;
pub type Conditions = ();

#[derive(Clone, Debug)]
pub struct TestOrderer {}
impl Orderer<MemberId, MessageId, GroupControlMessage<MemberId, Conditions>> for TestOrderer {
    type State = TestOrdererState;

    type Operation = TestOperation;

    type Error = Infallible;

    fn next_message(
        y: Self::State,
        payload: &GroupControlMessage<MemberId, Conditions>,
    ) -> Result<(Self::State, Self::Operation), Self::Error> {
        todo!()
    }

    fn queue(y: Self::State, message: &Self::Operation) -> Result<Self::State, Self::Error> {
        todo!()
    }

    fn next_ready_message(
        y: Self::State,
    ) -> Result<(Self::State, Option<Self::Operation>), Self::Error> {
        todo!()
    }
}

pub type TestOrdererState = ();
pub type TestResolver = StrongRemove<MemberId, MessageId, Conditions, TestOrderer>;
pub type TestGroupState = GroupCrdtState<MemberId, MessageId, Conditions, TestOrderer>;
pub type TestGroup = GroupCrdt<MemberId, MessageId, Conditions, TestOrderer>;
pub type TestGroupError = GroupCrdtError<MemberId, MessageId, Conditions, TestOrderer>;
