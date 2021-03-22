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

use serde::{Serialize, Deserialize};
use std::convert::{TryInto, TryFrom};
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::wasm_bindgen;

/// A special [`Result`] type for p2panda-rs handling errors dynamically.
type Result<T> = anyhow::Result<T>;

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

#[cfg(target_arch = "wasm32")]
mod wasm_utils {
    use std::panic;

    use console_error_panic_hook::hook as panic_hook;
    use wasm_bindgen::prelude::wasm_bindgen;

    /// Sets a [`panic hook`] for better error messages in NodeJS or web browser.
    ///
    /// [`panic hook`]: https://crates.io/crates/console_error_panic_hook
    #[wasm_bindgen(js_name = setWasmPanicHook)]
    pub fn set_wasm_panic_hook() {
        panic::set_hook(Box::new(panic_hook));
    }
}

#[derive(Serialize, Deserialize)]
struct SignEncodeResult {
    pub encoded_entry: String,
    pub encoded_message: String,
    pub entry_hash: String,
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(js_name = signEncode)]
pub fn sign_encode(private_key: String, current_message: String) -> wasm_bindgen::JsValue {
    // make key pair
    let key_pair = key_pair::KeyPair::from_private_key(private_key);

    // make entry
    let mut fields = atomic::MessageFields::new();
    fields
        .add("message", atomic::MessageValue::Text(current_message))
        .unwrap();
    let message =
        atomic::Message::new_create(atomic::Hash::new_from_bytes(vec![1, 2, 3]).unwrap(), fields)
            .unwrap();

    let message_encoded = atomic::MessageEncoded::try_from(&message).unwrap();

    // The first entry in a log doesn't need and cannot have references to previous entries
    let entry = atomic::Entry::new(&atomic::LogId::default(), &message, None, None, None).unwrap();

    // sign and encode
    let entry_signed: atomic::EntrySigned = (&entry, &key_pair).try_into().unwrap();

    wasm_bindgen::JsValue::from_serde(
        &SignEncodeResult {
            encoded_entry: entry_signed.as_str().into(),
            encoded_message: message_encoded.as_str().into(),
            entry_hash: entry_signed.hash().as_hex().into(),
        }
    ).unwrap()
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(js_name = decodeEntry)]
pub fn decode_entry(entry_encoded: String) -> String {
    let entry_signed = atomic::EntrySigned::new(&entry_encoded).unwrap();
    let entry: atomic::Entry = (&entry_signed, None).try_into().unwrap();
    format!("{:#?}", entry)
}

#[cfg(target_arch = "wasm32")]
pub use wasm_utils::set_wasm_panic_hook;
