// SPDX-License-Identifier: AGPL-3.0-or-later

pub mod decode;
pub mod encode;
mod encoded_header;
pub mod error;
#[allow(clippy::module_inception)]
mod header;
mod log_id;
mod seq_num;
mod signature;
pub mod traits;
pub mod validate;

pub use encoded_header::{EncodedEntry, SIGNATURE_SIZE};
pub use header::{Entry, EntryBuilder, Header};
pub use log_id::LogId;
pub use seq_num::{SeqNum, FIRST_SEQ_NUM};
pub use signature::Signature;
