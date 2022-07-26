// SPDX-License-Identifier: AGPL-3.0-or-later

use thiserror::Error;

#[derive(Error, Debug)]
pub enum EntryBuilderError {
    /// Handle errors from `EncodedOperation` struct.
    #[error("entry does not contain any operation")]
    OperationMissing,

    #[error(transparent)]
    EncodeEntryError(#[from] EncodeEntryError),
}

#[derive(Error, Debug)]
pub enum EncodeEntryError {
    /// Links should not be set when first entry in log.
    #[error("backlink and skiplink not valid for this sequence number")]
    InvalidLinks,

    /// Handle errors from encoding `bamboo_rs_core_ed25519_yasmf` entries.
    #[error(transparent)]
    BambooEncodeError(#[from] bamboo_rs_core_ed25519_yasmf::entry::encode::Error),
}

#[derive(Error, Debug)]
pub enum DecodeEntryError {
    /// Handle errors from `SeqNum` struct.
    #[error(transparent)]
    SeqNumError(#[from] SeqNumError),

    /// Handle errors from entry validation.
    #[error(transparent)]
    ValidateEntryError(#[from] ValidateEntryError),

    /// Handle errors from `Hash` struct.
    #[error(transparent)]
    HashError(#[from] crate::hash::HashError),

    /// Handle errors from decoding bamboo_rs_core_ed25519_yasmf entries.
    #[error(transparent)]
    BambooDecodeError(#[from] bamboo_rs_core_ed25519_yasmf::entry::decode::Error),
}

#[derive(Error, Debug)]
pub enum ValidateEntryError {
    /// Operation needs to match payload hash of encoded entry.
    #[error("operation needs to match payload hash of encoded entry")]
    PayloadHashMismatch,

    /// Operation needs to match payload size of encoded entry.
    #[error("operation needs to match payload size of encoded entry")]
    PayloadSizeMismatch,

    /// Invalid configuration of backlink and skiplink hashes for this sequence number.
    #[error("backlink and skiplink not valid for this sequence number")]
    InvalidLinks,

    /// Handle errors from `ed25519_dalek` crate.
    #[error(transparent)]
    Ed25519SignatureError(#[from] ed25519_dalek::SignatureError),
}

/// Custom error types for `SeqNum`.
#[derive(Error, Debug)]
pub enum SeqNumError {
    /// Sequence numbers are always positive.
    #[error("sequence number can not be zero or negative")]
    NotZeroOrNegative,

    /// Conversion to u64 from string failed.
    #[error("string contains invalid u64 value")]
    InvalidU64String,
}

/// Custom error types for `LogId`.
#[derive(Error, Debug)]
pub enum LogIdError {
    /// Conversion to u64 from string failed.
    #[error("string contains invalid u64 value")]
    InvalidU64String,
}
