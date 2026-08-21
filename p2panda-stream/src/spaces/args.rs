// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_spaces::SpacesMessage;

#[derive(Clone, Debug, Default)]
#[allow(clippy::large_enum_variant)]
pub enum SpacesProcessorArgs<L, TP, C> {
    Process {
        topic: TP,
        log_id: L,
        msg: SpacesMessage<C>,
    },
    #[default]
    Ignore,
}
