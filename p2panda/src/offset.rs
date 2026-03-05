// SPDX-License-Identifier: MIT OR Apache-2.0

#[derive(Clone, Default, Debug, PartialEq, Eq)]
pub enum Offset {
    /// Stream all events from the beginning, including already acknowledged ones.
    Start,

    /// Stream only unacknowledged events from where we've ended last.
    #[default]
    Frontier,
    // TODO: Later we add another variant named "Checkpoint" which holds a state vector.
}
