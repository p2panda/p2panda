// SPDX-License-Identifier: MIT OR Apache-2.0

// @TODO: Remove this later.
#![allow(dead_code)]

//! This crate provides an api for managing scoped message streams encrypted towards dynamic group of
//! users. The p2panda-encryption Data Encryption scheme is used for key agreement and group
//! management is achieved through an integration with p2panda-auth groups.
//! 
//! ## Features
//! 
//! * Decentralized group key agreement with post-compromise security and optional forward secrecy
//! * Decentralized group management with robust conflict resolution strategies
//! * Private space identifiers
//! * Re-use of groups across encryption boundaries
//! * Nested groups allowing for modelling multi-device profiles
//! * Generic over underlying data types
//! 
//! ## Requirements
//! 
//! * Messages must be ordered according to causal relations before processing
//! 
//! Read more about the underlying [groups CRDT](https://docs.rs/p2panda-auth/latest/p2panda_auth/)
//! and [encryption scheme](https://docs.rs/p2panda-encryption/latest/p2panda_encryption/).
pub mod auth;
pub mod config;
pub mod credentials;
pub mod encryption;
pub mod event;
pub mod group;
pub mod identity;
pub mod manager;
pub mod member;
pub mod message;
pub mod space;
#[cfg(any(test, feature = "test_utils"))]
pub mod test_utils;
#[cfg(test)]
mod tests;
pub mod traits;
pub mod types;
pub mod utils;

pub use config::Config;
pub use credentials::Credentials;
pub use types::{ActorId, OperationId};
