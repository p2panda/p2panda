use std::error::Error;
use std::fmt::Debug;

use serde::{Deserialize, Serialize};

/// Interface for processing messages which have particular ordering requirements.
///
/// Messages have a sender id, a unique identifier and a generic payload.
pub trait Ordering<MSG, P> {
    type State: Clone + Debug + Serialize + for<'a> Deserialize<'a>;

    type Error: Error;

    /// Create a next message with generic payload based on current local state, relevant
    /// meta-data is attached to the message.
    fn next_message(y: Self::State, payload: &P) -> Result<(Self::State, MSG), Self::Error>;

    /// Queue up a new local or remote message.
    fn queue(y: Self::State, message: &MSG) -> Result<Self::State, Self::Error>;

    /// Retrieve the next ready message.
    #[allow(clippy::type_complexity)]
    fn next_ready_message(y: Self::State) -> Result<(Self::State, Option<MSG>), Self::Error>;
}
