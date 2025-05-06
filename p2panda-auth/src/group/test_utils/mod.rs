mod group_store;
mod network;
mod orderer;

pub use group_store::{TestGroupStore, TestGroupStoreState};
pub use orderer::*;

use crate::traits::{IdentityHandle, OperationId};

use super::{Group, GroupState, GroupStateInner, resolver::GroupResolver};

impl IdentityHandle for char {}
impl OperationId for u32 {}

pub(crate) type TestResolver = GroupResolver<char, u32, TestOperation<char, u32>>;
pub(crate) type TestGroup = Group<char, u32, TestResolver, TestOrderer, TestGroupStore<char, TestGroupStateInner>>;
pub(crate) type TestGroupState =
    GroupState<char, u32, TestResolver, TestOrderer, TestGroupStore<char, TestGroupStateInner>>;
pub(crate) type TestGroupStateInner = GroupStateInner<char, u32, TestOperation<char, u32>>;
