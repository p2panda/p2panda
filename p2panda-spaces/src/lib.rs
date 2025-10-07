// SPDX-License-Identifier: MIT OR Apache-2.0

// @TODO: Remove this later.
#![allow(dead_code)]

//! This crate provides access-controlled and encrypted data spaces for dynamic groups of users.
//! This feature set is achieved through an integration of p2panda-access and p2panda-encryption.
//! Re-usable groups can be added to one or many spaces, each space has it's own private
//! identifier and encryption scope. The "Data Encryption" scheme is used which provides
//! post-compromise security and optional forward secrecy.
//! 
//! Read more about the underlying [groups
//! CRDT](https://docs.rs/p2panda-auth/latest/p2panda_auth/) and [encryption
//! scheme](https://docs.rs/p2panda-encryption/latest/p2panda_encryption/).
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
