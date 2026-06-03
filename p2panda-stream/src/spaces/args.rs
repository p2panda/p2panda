// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_spaces::SpacesMessage;

#[derive(Clone, Debug, Default)]
#[allow(clippy::large_enum_variant)]
pub enum SpacesArgs<ID, C> {
    Process {
        msg: SpacesMessage<ID, C>,
    },
    #[default]
    Ignore,
}
