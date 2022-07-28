// SPDX-License-Identifier: AGPL-3.0-or-later

//! Create, sign, encode and decode [`Bamboo`] entries.
//!
//! Bamboo entries are the main data type of p2panda. Entries are organised in a distributed,
//! single-writer append-only log structure, created and signed by holders of private keys and
//! stored inside the node's database.
//!
//! Entries contain p2panda operations as their payload, giving them security against malicious
//! behaviour in a distributed system and authenticity with digital signatures.
//!
//! ## Decoding
//!
//! Entries can be created programmatically via the API (`EntryBuilder`) or decoded from raw bytes
//! (`EncodedEntry`). In both cases different validation steps need to be applied to make sure the
//! entry is well formed.
//!
//! Use the `EntryBuilder` to create an `Entry` instance through the API. It serves as an interface
//! to set the entry arguments and the `Operation` payload and to sign it with a private `KeyPair`
//! which will result in the final `Entry`.
//!
//! To derive an `Entry` from bytes, use the `EncodedEntry` struct which allows you to decode the
//! data into the final `Entry`.
//!
//! Here is an overview of the methods to create or decode an entry:
//!
//! ```text
//!             ┌────────────┐                         ┌─────┐
//!  bytes ───► │EncodedEntry│ ────decode_entry()────► │Entry│
//!             └────────────┘                         └─────┘
//! ┌───────┐                                             ▲
//! │KeyPair│ ──────────┐                                 │
//! └───────┘           │                                 │
//!                     │                                 │
//! ┌────────────┐      ▼                                 │
//! │EntryBuilder├────sign()──────────────────────────────┘
//! └────────────┘
//! ```
//!
//! Please note that `Entry` in itself is immutable and can not directly be deserialized, there are
//! only these above mentioned approaches to arrive at it. Both approaches apply all means to
//! validate the integrity and correct encoding of the entry as per specification.
//!
//! ## Encoding
//!
//! `Entry` structs can be encoded again into their raw bytes form like that:
//!
//! ```text
//! ┌─────┐                     ┌────────────┐
//! │Entry│ ──encode_entry()──► │EncodedEntry│ ─────► bytes
//! └─────┘                     └────────────┘
//! ```
//!
//! ## Validation
//!
//! The above high-level methods will automatically do different sorts of validation checks. All
//! low-level methods can also be used independently, depending on your implementation:
//!
//! 1. Correct hexadecimal encoding (when using human-readable encoding format) (#E1)
//! 2. Correct Bamboo encoding as per specification (#E2)
//! 3. Check if back- and skiplinks are correctly set for given sequence number (#E3)
//! 4. Verify log-integrity (matching back- & skiplink entries, author, log id) (#E4)
//! 5. Verify signature (#E5)
//! 6. Check if payload matches claimed hash and size (#E6)
//!
//! Please note that almost all validation can take place without any external information, except
//! of checking for log integrity. This requires some sort of persistence layer where past entries
//! are stored which is not in scope of this crate. You can check this manually with the help of
//! the `validate_log_integrity` method.
//!
//! See `operations` and `schema` module for more validation methods around operations (#E6). There
//! you will also find the complete flow which will cover validation on both entries and
//! operations, applying all checks required as per specification.
//!
//! [`Bamboo`]: https://github.com/AljoschaMeyer/bamboo
pub mod decode;
pub mod encode;
mod encoded_entry;
#[allow(clippy::module_inception)]
mod entry;
pub mod error;
mod log_id;
mod seq_num;
mod signature;
#[cfg(test)]
mod tests;
pub mod validate;

pub use encoded_entry::EncodedEntry;
pub use entry::{Entry, EntryBuilder};
pub use log_id::LogId;
pub use seq_num::SeqNum;
pub use signature::Signature;
