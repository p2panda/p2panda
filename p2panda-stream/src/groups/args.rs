// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_core::{Hash, Operation};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[allow(clippy::large_enum_variant)]
pub enum GroupsArgs<E> {
    Process {
        state_id: Hash,
        operation: Operation<E>,
    },
    #[default]
    Ignore,
}
