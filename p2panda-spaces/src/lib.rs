// SPDX-License-Identifier: MIT OR Apache-2.0

#![cfg_attr(docsrs, feature(doc_cfg))]

mod auth;
mod config;
mod credentials;
mod encryption;
mod event;
pub mod group;
pub mod identity;
pub mod manager;
pub mod member;
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
pub use event::{Event, GroupActor, GroupContext, GroupEvent, SpaceContext, SpaceEvent};
pub use message::{SpacesArgs, SpacesMessage};
pub use types::{ActorId, OperationId, StrongRemoveResolver};
