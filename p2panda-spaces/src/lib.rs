// SPDX-License-Identifier: MIT OR Apache-2.0

#![cfg_attr(docsrs, feature(doc_cfg))]

mod auth;
mod config;
mod credentials;
mod encryption;
mod event;
mod forge;
pub mod group;
pub mod identity;
pub mod manager;
mod member;
mod message;
pub mod space;
mod store;
#[cfg(any(test, feature = "test_utils"))]
pub mod test_utils;
#[cfg(test)]
mod tests;
mod types;
mod utils;

use p2panda_core::{Hash, VerifyingKey};

pub use auth::message::AuthMessage;
pub use config::Config;
pub use credentials::Credentials;
pub use event::Event;
pub use forge::Forge;
pub use message::{SpacesArgs, SpacesMessage};
pub use store::SpacesStoreState;
pub use types::StrongRemoveResolver;

pub type SpaceId = Hash;

pub type GroupId = ActorId;

pub type MemberId = ActorId;

pub type ActorId = VerifyingKey;

pub type OperationId = Hash;
