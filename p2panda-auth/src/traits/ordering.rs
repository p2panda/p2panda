use std::error::Error;
use std::fmt::Debug;

use serde::{Deserialize, Serialize};

use super::Operation;

/// Interface for processing messages which have particular ordering requirements.
///
/// Messages have a sender id, a unique identifier and a generic payload.
pub trait Ordering<ID, OP, P> {
    type State: Clone + Debug + Serialize + for<'a> Deserialize<'a>;

    type Message: Clone + Debug + Operation<ID, OP, P> + Serialize + for<'a> Deserialize<'a>;

    type Error: Error;

    /// Create a next message with generic payload based on current local state, relevant
    /// meta-data is attached to the message.
    fn next_message(
        y: Self::State,
        dependencies: Vec<OP>,
        payload: &P,
    ) -> Result<(Self::State, Self::Message), Self::Error>;

    /// Queue up a new local or remote message.
    fn queue(y: Self::State, message: &Self::Message) -> Result<Self::State, Self::Error>;

    /// Retrieve the next ready message.
    #[allow(clippy::type_complexity)]
    fn next_ready_message(
        y: Self::State,
    ) -> Result<(Self::State, Option<Self::Message>), Self::Error>;
}
