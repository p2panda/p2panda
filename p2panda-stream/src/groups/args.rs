// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_core::Operation;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[allow(clippy::large_enum_variant)]
pub enum GroupsProcessorArgs<SID, E> {
    Process {
        state_id: SID,
        operation: Operation<E>,
    },
    #[default]
    Ignore,
}
