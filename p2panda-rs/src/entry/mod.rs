// SPDX-License-Identifier: AGPL-3.0-or-later

//! Create, sign, encode and decode [`Bamboo`] entries.
//!
//! Bamboo entries are the main data type of p2panda. Entries are organised in a distributed,
//! single-writer append-only log structure, created and signed by holders of private keys and
//! stored inside the node database.
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
