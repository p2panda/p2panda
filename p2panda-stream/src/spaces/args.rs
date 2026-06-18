// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_spaces::SpacesMessage;

#[derive(Clone, Debug, Default)]
#[allow(clippy::large_enum_variant)]
pub enum SpacesProcessorArgs<C> {
    Process {
        msg: SpacesMessage<C>,
    },
    #[default]
    Ignore,
}
