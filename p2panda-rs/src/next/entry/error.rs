// SPDX-License-Identifier: AGPL-3.0-or-later

//! Error types for creating, encoding, decoding or validating entries and their regarding data
//! types like sequence numbers or log ids.
use thiserror::Error;

/// Errors from `EntryBuilder` struct.
#[derive(Error, Debug)]
pub enum EntryBuilderError {
    /// Handle errors from `entry::encode` module.
    #[error(transparent)]
    EncodeEntryError(#[from] EncodeEntryError),
}

/// Errors from `entry::encode` module.
#[derive(Error, Debug)]
pub enum EncodeEntryError {
    /// Handle errors from `entry::validate` module.
    #[error(transparent)]
    ValidateEntryError(#[from] ValidateEntryError),

    /// Handle errors from encoding `bamboo_rs_core_ed25519_yasmf` entries.
    #[error(transparent)]
    BambooEncodeError(#[from] bamboo_rs_core_ed25519_yasmf::entry::encode::Error),
}

/// Errors from `entry::decode` module.
#[derive(Error, Debug)]
pub enum DecodeEntryError {
    /// Handle errors from `entry::validate` module.
    #[error(transparent)]
    ValidateEntryError(#[from] ValidateEntryError),

    /// Handle errors from decoding `bamboo_rs_core_ed25519_yasmf` entries.
    #[error(transparent)]
    BambooDecodeError(#[from] bamboo_rs_core_ed25519_yasmf::entry::decode::Error),
}

/// Errors from `entry::validate` module.
#[derive(Error, Debug)]
#[allow(missing_copy_implementations)]
pub enum ValidateEntryError {
    /// Invalid configuration of backlink and skiplink hashes for this sequence number.
    #[error("backlink and skiplink not valid for this sequence number")]
    InvalidLinks,

    /// Operation needs to match payload hash of encoded entry.
    #[error("operation needs to match payload hash of encoded entry")]
    PayloadHashMismatch,

    /// Operation needs to match payload size of encoded entry.
    #[error("operation needs to match payload size of encoded entry")]
    PayloadSizeMismatch,
}

/// Errors from `SeqNum` struct.
#[derive(Error, Debug)]
#[allow(missing_copy_implementations)]
pub enum SeqNumError {
    /// Sequence numbers are always positive.
    #[error("sequence number can not be zero or negative")]
    NotZeroOrNegative,

    /// Conversion to u64 from string failed.
    #[error("string contains invalid u64 value")]
    InvalidU64String,
}

/// Errors from `LogId` struct.
#[derive(Error, Debug)]
#[allow(missing_copy_implementations)]
pub enum LogIdError {
    /// Conversion to u64 from string failed.
    #[error("string contains invalid u64 value")]
    InvalidU64String,
}
