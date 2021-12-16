// SPDX-License-Identifier: AGPL-3.0-or-later

//! # p2panda-rs
//!
//! This library provides all tools required to write a client for the [p2panda] network. It is
//! shipped both as a Rust crate `p2panda-rs` with WebAssembly bindings and a NPM package
//! `p2panda-js` with TypeScript definitions running in NodeJS or any modern web browser.
//!
//! [p2panda]: https://p2panda.org
//!
//! ## Example
//!
//! Creates and signs data which can be sent to a p2panda node.
//!
//! ```
//! # extern crate p2panda_rs;
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! # use std::convert::TryFrom;
//! # use p2panda_rs::entry::{sign_and_encode, Entry, EntrySigned, LogId, SeqNum};
//! # use p2panda_rs::hash::Hash;
//! # use p2panda_rs::identity::KeyPair;
//! # use p2panda_rs::operation::{Operation, OperationFields, OperationValue};
//! # let profile_schema = Hash::new_from_bytes(vec![1, 2, 3])?;
//! // Generate new Ed25519 key pair
//! let key_pair = KeyPair::new();
//!
//! // Create operation fields which contain the data we want to send
//! let mut fields = OperationFields::new();
//! fields.add("username", OperationValue::Text("panda".to_owned()))?;
//!
//! // Add field data to "create" operation
//! let operation = Operation::new_create(profile_schema, fields)?;
//!
//! // This is the entry at sequence number 1 (the first entry in the log)
//! let seq_num = SeqNum::new(1)?;
//!
//! // Wrap operation into Bamboo entry (append-only log data type)
//! let entry = Entry::new(&LogId::default(), Some(&operation), None, None, &seq_num)?;
//!
//! // Sign entry with private key
//! let entry_signed = sign_and_encode(&entry, &key_pair)?;
//! # Ok(())
//! # }
//! ```
#![warn(
    missing_copy_implementations,
    missing_debug_implementations,
    missing_doc_code_examples,
    missing_docs,
    trivial_casts,
    trivial_numeric_casts,
    unsafe_code,
    unstable_features,
    unused_import_braces,
    unused_qualifications
)]
// This must be imported here at the root of the crate in order for the fixture macros to work as expected.
#![allow(clippy::single_component_path_imports)]
#[cfg(test)]
use rstest_reuse;

pub mod entry;
pub mod hash;
pub mod identity;
pub mod instance;
pub mod operation;
pub mod schema;
pub mod secret_group;
pub mod test_utils;
#[cfg(target_arch = "wasm32")]
pub mod wasm;

/// Trait used by p2panda structs to validate arguments.
pub trait Validate {
    /// Validation error type.
    type Error;

    /// Validates p2panda data type instance.
    fn validate(&self) -> Result<(), Self::Error>;
}
