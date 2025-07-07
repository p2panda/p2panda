// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::traits::Operation;
use std::error::Error;

/// Interface for processing messages which have particular ordering requirements.
///
/// Messages have an author id, a unique identifier and a generic payload.
pub trait Orderer<ID, OP, P> {
    type State;
    type Operation: Operation<ID, OP, P>;
    type Error: Error;

    /// Create a next message with generic payload based on current local state, relevant
    /// meta-data is attached to the message.
    fn next_message(
        y: Self::State,
        payload: &P,
    ) -> Result<(Self::State, Self::Operation), Self::Error>;

    /// Queue up a new local or remote message.
    fn queue(y: Self::State, message: &Self::Operation) -> Result<Self::State, Self::Error>;

    /// Retrieve the next ready message.
    #[allow(clippy::type_complexity)]
    fn next_ready_message(
        y: Self::State,
    ) -> Result<(Self::State, Option<Self::Operation>), Self::Error>;
}
