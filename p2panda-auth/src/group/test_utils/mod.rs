// SPDX-License-Identifier: MIT OR Apache-2.0

mod group_store;
mod network;
mod orderer;
mod partial_ord;

pub use group_store::TestGroupStore;
pub use network::Network;
pub use orderer::*;
pub use partial_ord::*;

use crate::traits::{IdentityHandle, OperationId};

use super::{Group, GroupState, resolver::GroupResolver};

impl IdentityHandle for char {}
impl OperationId for u32 {}

pub type MemberId = char;
pub type GroupId = char;
pub type MessageId = u32;

pub type GenericTestResolver<ORD, GS> = GroupResolver<MemberId, MessageId, ORD, GS>;
pub type GenericTestGroup<RS, ORD, GS> = Group<MemberId, MessageId, RS, ORD, GS>;
pub type GenericTestGroupState<RS, ORD, GS> = GroupState<MemberId, MessageId, RS, ORD, GS>;

pub type TestResolver = GenericTestResolver<TestOrderer, TestGroupStore<MemberId>>;
pub type TestGroup = GenericTestGroup<TestResolver, TestOrderer, TestGroupStore<MemberId>>;
pub type TestGroupState =
    GenericTestGroupState<TestResolver, TestOrderer, TestGroupStore<MemberId>>;
