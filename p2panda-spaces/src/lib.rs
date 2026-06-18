// SPDX-License-Identifier: MIT OR Apache-2.0

#![cfg_attr(docsrs, feature(doc_cfg))]

mod auth;
mod config;
mod credentials;
mod encryption;
mod event;
pub mod forge;
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

pub use auth::message::AuthMessage;
pub use config::Config;
pub use credentials::Credentials;
pub use event::Event;
pub use message::{SpacesArgs, SpacesMessage};
pub use store::StoreError;
pub use types::StrongRemoveResolver;
