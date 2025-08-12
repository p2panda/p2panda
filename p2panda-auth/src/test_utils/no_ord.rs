// SPDX-License-Identifier: MIT OR Apache-2.0

use std::convert::Infallible;
use std::fmt::Debug;

use crate::group::GroupControlMessage;
use crate::group::{GroupCrdt, GroupCrdtError, GroupCrdtState};
use crate::test_utils::{Conditions, MemberId, MessageId, TestOperation, TestResolver};
use crate::traits::Orderer;

#[derive(Clone, Debug)]
pub struct TestOrderer {}
impl Orderer<MemberId, MessageId, GroupControlMessage<MemberId, Conditions>> for TestOrderer {
    type State = TestOrdererState;

    type Operation = TestOperation;

    type Error = Infallible;

    fn next_message(
        _y: Self::State,
        _payload: &GroupControlMessage<MemberId, Conditions>,
    ) -> Result<(Self::State, Self::Operation), Self::Error> {
        unimplemented!()
    }

    fn queue(_y: Self::State, _message: &Self::Operation) -> Result<Self::State, Self::Error> {
        unimplemented!()
    }

    fn next_ready_message(
        _y: Self::State,
    ) -> Result<(Self::State, Option<Self::Operation>), Self::Error> {
        unimplemented!()
    }
}

pub type TestOrdererState = ();
pub type TestGroupState = GroupCrdtState<MemberId, MessageId, Conditions, TestOrderer>;
pub type TestGroup = GroupCrdt<MemberId, MessageId, Conditions, TestResolver, TestOrderer>;
pub type TestGroupError =
    GroupCrdtError<MemberId, MessageId, Conditions, TestResolver, TestOrderer>;
