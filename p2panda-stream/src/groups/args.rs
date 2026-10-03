// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_spaces::AuthMessage;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[allow(clippy::large_enum_variant)]
pub enum GroupsProcessorArgs<C> {
    Process {
        message: AuthMessage<C>,
    },
    #[default]
    Ignore,
}
