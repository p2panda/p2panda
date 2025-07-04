// SPDX-License-Identifier: MIT OR Apache-2.0

mod group_store;
mod network;
mod orderer;
mod partial_ord;

pub use crate::group::test_utils::group_store::TestGroupStore;
pub use network::Network;
pub use orderer::*;
pub use partial_ord::*;

use crate::group::resolver::StrongRemove;
use crate::group::{Access, Group, GroupState};
use crate::traits::{IdentityHandle, OperationId};

impl IdentityHandle for char {}
impl OperationId for u32 {}

pub type MemberId = char;
pub type GroupId = char;
pub type MessageId = u32;
pub type Conditions = ();

pub type GenericTestResolver<ORD, GS> = StrongRemove<MemberId, MessageId, Conditions, ORD, GS>;
pub type GenericTestGroup<RS, ORD, GS> = Group<MemberId, MessageId, Conditions, RS, ORD, GS>;
pub type GenericTestGroupState<RS, ORD, GS> =
    GroupState<MemberId, MessageId, Conditions, RS, ORD, GS>;

pub type TestResolver = GenericTestResolver<TestOrderer, TestGroupStore>;
pub type TestGroup = GenericTestGroup<TestResolver, TestOrderer, TestGroupStore>;
pub type TestGroupState = GenericTestGroupState<TestResolver, TestOrderer, TestGroupStore>;

/// During testing we want Ord to be implemented on Access so we can easily assert test cases
/// involving collections of access levels.
impl<C: Ord> Ord for Access<C> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        let level_cmp = self.level.cmp(&other.level);
        if level_cmp != std::cmp::Ordering::Equal {
            level_cmp
        } else {
            self.conditions.cmp(&other.conditions)
        }
    }
}
