// SPDX-License-Identifier: MIT OR Apache-2.0

pub mod spaces_store;
pub mod key_store;
pub mod message;

use std::fmt::Debug;

use serde::Serialize;
use serde::de::DeserializeOwned;

/// Trait representing the identifier of a space.
pub trait SpaceId: Debug + Copy + Eq + PartialEq + DeserializeOwned + Serialize {}
