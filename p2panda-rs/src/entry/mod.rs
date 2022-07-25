// SPDX-License-Identifier: AGPL-3.0-or-later

//! Create, sign, encode and decode [`Bamboo`] entries.
//!
//! Bamboo entries are the main data type of p2panda. Entries are organised in a distributed,
//! single-writer append-only log structure, created and signed by holders of private keys and
//! stored inside the node database.
//!
//! [`Bamboo`]: https://github.com/AljoschaMeyer/bamboo
mod decode;
mod encode;
mod encoded_entry;
#[allow(clippy::module_inception)]
mod entry;
mod error;
mod log_id;
mod seq_num;
#[cfg(test)]
mod tests;
mod validate;

pub use decode::decode_entry;
pub use encode::{encode_entry, sign_entry};
pub use encoded_entry::EncodedEntry;
pub use entry::{Entry, EntryBuilder};
pub use error::{EntryBuilderError, EntryError, EntrySignedError, LogIdError, SeqNumError};
pub use log_id::LogId;
pub use seq_num::SeqNum;
pub use validate::verify_payload;
