// SPDX-License-Identifier: AGPL-3.0-or-later

//! Create, sign, encode and decode [`Bamboo`] entries.
//!
//! Bamboo entries are the main data type of p2panda. Entries are organized in a distributed,
//! single-writer append-only log structure, created and signed by holders of private keys and
//! stored inside the node database.
//!
//! [`Bamboo`]: https://github.com/AljoschaMeyer/bamboo
mod decode;
mod encode;
mod entry;
mod entry_signed;
mod error;
mod log_id;
mod seq_num;

pub use decode::decode_entry;
pub use encode::sign_and_encode;
pub use entry::Entry;
pub use entry_signed::EntrySigned;
pub use error::{EntryError, EntrySignedError, SeqNumError};
pub use log_id::LogId;
pub use seq_num::SeqNum;
