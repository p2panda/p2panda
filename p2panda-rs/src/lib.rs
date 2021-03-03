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
//! # extern crate anyhow;
//! # fn main() -> Result<(), anyhow::Error> {
//! # use std::convert::TryFrom;
//! # use p2panda_rs::key_pair::KeyPair;
//! # use p2panda_rs::atomic::{Entry, EntrySigned, Hash, LogId, SeqNum, Message, MessageFields, MessageValue};
//! # let PROFILE_SCHEMA = Hash::new_from_bytes(vec![1, 2, 3])?;
//! // Generate new Ed25519 key pair
//! let key_pair = KeyPair::new();
//!
//! // Create message fields which contain the data we want to send
//! let mut fields = MessageFields::new();
//! fields.add("username", MessageValue::Text("panda".to_owned()))?;
//!
//! // Add field data to "create" message
//! let message = Message::new_create(PROFILE_SCHEMA, fields)?;
//!
//! // Wrap message into Bamboo entry (append-only log data type)
//! let entry = Entry::new(&LogId::default(), &message, None, None, None)?;
//!
//! // Sign entry with private key
//! let entry_signed = EntrySigned::try_from((&entry, &key_pair))?;
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

/// A special `Result` type for p2panda-rs handling errors dynamically.
type Result<T> = anyhow::Result<T>;

/// Basic structs and methods to interact with p2panda data structures.
pub mod atomic;
/// Methods to generate key pairs or "authors" to sign data with.
pub mod key_pair;
/// Validations for message payloads and definitions of system schemas.
///
/// This uses [Concise Data Definition Language (CDDL)](https://tools.ietf.org/html/rfc8610)
/// internally to verify CBOR data of p2panda messages.
pub mod schema;

#[cfg(target_arch = "wasm32")]
mod wasm_utils {
    use std::panic;

    use console_error_panic_hook::hook as panic_hook;
    use wasm_bindgen::prelude::wasm_bindgen;

    /// Sets a panic hook for better error messages in NodeJS or web browser. See:
    /// https://crates.io/crates/console_error_panic_hook
    #[wasm_bindgen(js_name = setWasmPanicHook)]
    pub fn set_wasm_panic_hook() {
        panic::set_hook(Box::new(panic_hook));
    }
}

#[cfg(target_arch = "wasm32")]
pub use wasm_utils::set_wasm_panic_hook;
