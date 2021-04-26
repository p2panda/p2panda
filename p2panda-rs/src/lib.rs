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
//! # use p2panda_rs::key_pair::KeyPair;
//! # use p2panda_rs::atomic::{Entry, EntrySigned, Hash, LogId, SeqNum, Message, MessageFields, MessageValue};
//! # let profile_schema = Hash::new_from_bytes(vec![1, 2, 3])?;
//! // Generate new Ed25519 key pair
//! let key_pair = KeyPair::new();
//!
//! // Create message fields which contain the data we want to send
//! let mut fields = MessageFields::new();
//! fields.add("username", MessageValue::Text("panda".to_owned()))?;
//!
//! // Add field data to "create" message
//! let message = Message::new_create(profile_schema, fields)?;
//!
//! // This is the entry at sequence number 1 (the first entry in the log)
//! let seq_num = SeqNum::new(1)?;
//!
//! // Wrap message into Bamboo entry (append-only log data type)
//! let entry = Entry::new(&LogId::default(), Some(&message), None, None, &seq_num)?;
//!
//! // Sign entry with private key
//! let entry_signed = entry.sign(&key_pair)?;
//! # Ok(())
//! # }
//! ```
#![warn(
    missing_copy_implementations,
    missing_debug_implementations,
    missing_docs,
    trivial_casts,
    trivial_numeric_casts,
    unsafe_code,
    unstable_features,
    unused_import_braces,
    unused_qualifications
)]

/// Basic structs and methods to interact with p2panda data structures.
pub mod atomic;
/// Methods to generate key pairs or "authors" to sign data with.
pub mod key_pair;
/// Validations for message payloads and definitions of system schemas.
///
/// This uses [`Concise Data Definition Language`] (CDDL) internally to verify CBOR data of p2panda
/// messages.
///
/// [`Concise Data Definition Language`]: https://tools.ietf.org/html/rfc8610
pub mod schema;
/// Methods exported for WebAssembly targets.
///
/// Wrappers for these methods are available in [p2panda-js], which allows idiomatic
/// usage of `p2panda-rs` in a Javascript/Typescript environment.
///
/// [p2panda-js]: https://github.com/p2panda/p2panda/tree/main/p2panda-js
#[cfg(target_arch = "wasm32")]
pub mod wasm;
