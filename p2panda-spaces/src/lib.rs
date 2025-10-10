// SPDX-License-Identifier: MIT OR Apache-2.0

#![doc = include_str!("../README.md")]
mod auth;
mod config;
mod credentials;
mod encryption;
mod event;
pub mod group;
pub mod identity;
pub mod manager;
mod member;
mod message;
pub mod space;
#[cfg(any(test, feature = "test_utils"))]
pub mod test_utils;
#[cfg(test)]
mod tests;
pub mod traits;
mod types;
mod utils;

pub use config::Config;
pub use credentials::Credentials;
pub use event::Event;
pub use message::SpacesArgs;
pub use types::{ActorId, OperationId};
