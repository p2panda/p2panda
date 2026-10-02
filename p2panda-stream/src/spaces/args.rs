// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_spaces::{Event, SpacesMessage};

#[derive(Clone, Debug, Default)]
#[allow(clippy::large_enum_variant)]
pub enum SpacesProcessorArgs<C> {
    Process {
        msg: SpacesMessage<C>,
    },
    AlreadyProcessed {
        msg: SpacesMessage<C>,
        events: Vec<Event<C>>,
    },
    #[default]
    Ignore,
}
