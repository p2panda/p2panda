// SPDX-License-Identifier: AGPL-3.0-or-later

//! Methods exported for WebAssembly targets.
//!
//! Wrappers for these methods are available in [p2panda-js], which allows idiomatic usage of
//! `p2panda-rs` in a JavaScript/TypeScript environment.
//!
//! [p2panda-js]: https://github.com/p2panda/p2panda/tree/main/p2panda-js
mod entry;
pub mod error;
mod key_pair;
mod operation;
mod serde;
#[cfg(test)]
mod tests;

pub use entry::{decode_entry, sign_encode_entry, SignEncodeEntryResult};
pub use key_pair::{verify_signature, KeyPair};
pub use operation::{
    encode_create_operation, encode_delete_operation, encode_update_operation, OperationFields,
};
