// SPDX-License-Identifier: MIT OR Apache-2.0

//! Traits interfaces.
mod forge;
mod message;
mod store;

use std::fmt::Debug;

use serde::Serialize;
use serde::de::DeserializeOwned;

pub use forge::Forge;
pub use message::AuthoredMessage;
pub use store::{AuthStore, MessageStore, SpacesStore};

/// Trait representing the identifier of a space.
pub trait SpaceId:
    Debug + Copy + Eq + PartialEq + std::hash::Hash + DeserializeOwned + Serialize
{
}
