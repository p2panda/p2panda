// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_core::Hash;

use crate::groups::GroupsOperation;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[allow(clippy::large_enum_variant)]
pub enum GroupsArgs<C> {
    Process {
        state_id: Hash,
        operation: GroupsOperation<C>,
    },
    #[default]
    Ignore,
}
