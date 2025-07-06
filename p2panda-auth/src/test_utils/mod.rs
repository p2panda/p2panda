// SPDX-License-Identifier: MIT OR Apache-2.0

mod group_store;
mod network;
mod orderer;
mod partial_ord;

pub use crate::test_utils::group_store::TestGroupStore;
pub use network::Network;
pub use orderer::*;
pub use partial_ord::*;

use crate::group::resolver::StrongRemove;
use crate::group::{GroupCrdt, GroupCrdtState, GroupError};
use crate::traits::{IdentityHandle, OperationId};

impl IdentityHandle for char {}
impl OperationId for u32 {}

pub type MemberId = char;
pub type MessageId = u32;
pub type Conditions = ();

pub type GenericTestResolver<ORD, GS> = StrongRemove<MemberId, MessageId, Conditions, ORD, GS>;
pub type GenericTestGroup<RS, ORD, GS> = GroupCrdt<MemberId, MessageId, Conditions, RS, ORD, GS>;
pub type GenericTestGroupState<RS, ORD, GS> =
    GroupCrdtState<MemberId, MessageId, Conditions, RS, ORD, GS>;

pub type TestResolver = GenericTestResolver<TestOrderer, TestGroupStore>;
pub type TestGroup = GenericTestGroup<TestResolver, TestOrderer, TestGroupStore>;
pub type TestGroupState = GenericTestGroupState<TestResolver, TestOrderer, TestGroupStore>;
pub type TestGroupError =
    GroupError<MemberId, MessageId, Conditions, TestResolver, TestOrderer, TestGroupStore>;
