// SPDX-License-Identifier: MIT OR Apache-2.0

//! This crate provides an API for managing scoped message streams encrypted towards a dynamic
//! group of actors. The p2panda-encryption Data Encryption scheme is used for key agreement and
//! group management is achieved through an integration with p2panda-auth groups.
//!
//! ## Features
//!
//! * Decentralized group key agreement with forward secrecy and encrypted messaging with
//!   post-compromise security
//! * Decentralized group management with robust conflict resolution strategies
//! * Private space identifiers
//! * Re-use of groups across encryption boundaries
//! * Nested groups allowing for modelling multi-device profiles
//! * Generic over message type
//!
//! ## Requirements
//!
//! * Messages must be ordered according to causal relations
//! * Messages must be signed and verified
//!
//! Read more about the underlying [groups
//! CRDT](https://docs.rs/p2panda-auth/latest/p2panda_auth/) and [encryption
//! scheme](https://docs.rs/p2panda-encryption/latest/p2panda_encryption/).
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
