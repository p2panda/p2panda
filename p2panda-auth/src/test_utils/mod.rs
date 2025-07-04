// SPDX-License-Identifier: MIT OR Apache-2.0

mod group_store;
mod network;
mod orderer;
mod partial_ord;

pub(crate) use crate::test_utils::group_store::TestGroupStore;
pub(crate) use network::Network;
pub(crate) use orderer::*;
pub(crate) use partial_ord::*;

use crate::group::resolver::StrongRemove;
use crate::group::{GroupCrdt, GroupCrdtState};
use crate::traits::{IdentityHandle, OperationId};

impl IdentityHandle for char {}
impl OperationId for u32 {}

pub(crate) type MemberId = char;
pub(crate) type MessageId = u32;
pub(crate) type Conditions = ();

pub(crate) type GenericTestResolver<ORD, GS> =
    StrongRemove<MemberId, MessageId, Conditions, ORD, GS>;
pub(crate) type GenericTestGroup<RS, ORD, GS> =
    GroupCrdt<MemberId, MessageId, Conditions, RS, ORD, GS>;
pub(crate) type GenericTestGroupState<RS, ORD, GS> =
    GroupCrdtState<MemberId, MessageId, Conditions, RS, ORD, GS>;

pub(crate) type TestResolver = GenericTestResolver<TestOrderer, TestGroupStore>;
pub(crate) type TestGroup = GenericTestGroup<TestResolver, TestOrderer, TestGroupStore>;
pub(crate) type TestGroupState = GenericTestGroupState<TestResolver, TestOrderer, TestGroupStore>;
