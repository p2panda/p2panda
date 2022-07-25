// SPDX-License-Identifier: AGPL-3.0-or-later

use thiserror::Error;

#[derive(Error, Debug)]
pub enum EntryBuilderError {
    /// Handle errors from `EncodedOperation` struct.
    #[error("entry does not contain any operation")]
    OperationMissing,

    /// Handle errors from `EncodedOperation` struct.
    #[error(transparent)]
    EncodedOperationError(#[from] crate::operation::EncodedOperationError),

    /// Handle errors from `EntrySigned` struct.
    #[error(transparent)]
    EntrySignedError(#[from] EntrySignedError),
}

/// Error types for methods of `Entry` struct.
#[allow(missing_copy_implementations)]
#[derive(Error, Debug)]
pub enum EntryError {
    /// Handle errors from `Hash` struct.
    #[error(transparent)]
    HashError(#[from] crate::hash::HashError),

    /// Handle errors from `SeqNum` struct.
    #[error(transparent)]
    SeqNumError(#[from] SeqNumError),
}

/// Custom error types for `EntrySigned`.
#[derive(Error, Debug)]
#[allow(missing_copy_implementations)]
pub enum EntrySignedError {
    /// Links should not be set when first entry in log.
    #[error("backlink and skiplink not valid for this sequence number")]
    InvalidLinks,

    /// Encoded entry string contains invalid hex characters.
    #[error("invalid hex encoding in entry")]
    InvalidHexEncoding,

    /// Operation needs to match payload hash of encoded entry.
    #[error("operation needs to match payload hash of encoded entry")]
    OperationHashMismatch,

    /// Can not sign and encode an entry without a `Operation`.
    #[error("entry does not contain any operation")]
    OperationMissing,

    /// Backlink is required for entry encoding.
    #[error("entry requires backlink for encoding")]
    BacklinkMissing,

    /// Skiplink is required for entry encoding.
    #[error("entry requires skiplink for encoding")]
    SkiplinkMissing,

    /// Backlink and skiplink hashes should be different.
    #[error("backlink and skiplink are identical")]
    BacklinkAndSkiplinkIdentical,

    /// Handle errors from `Entry` struct.
    #[error(transparent)]
    EntryError(#[from] EntryError),

    /// Handle errors from `SeqNum` struct.
    #[error(transparent)]
    SeqNumError(#[from] SeqNumError),

    /// Handle errors from `Hash` struct.
    #[error(transparent)]
    HashError(#[from] crate::hash::HashError),

    /// Handle errors from `EncodedOperation` struct.
    #[error(transparent)]
    EncodedOperationError(#[from] crate::operation::EncodedOperationError),

    /// Handle errors from encoding bamboo_rs_core_ed25519_yasmf entries.
    #[error(transparent)]
    BambooEncodeError(#[from] bamboo_rs_core_ed25519_yasmf::entry::encode::Error),

    /// Handle errors from decoding bamboo_rs_core_ed25519_yasmf entries.
    #[error(transparent)]
    BambooDecodeError(#[from] bamboo_rs_core_ed25519_yasmf::entry::decode::Error),

    /// Handle errors from ed25519_dalek crate.
    #[error(transparent)]
    Ed25519SignatureError(#[from] ed25519_dalek::SignatureError),
}

/// Custom error types for `SeqNum`.
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

/// Custom error types for `LogId`.
#[derive(Error, Debug)]
#[allow(missing_copy_implementations)]
pub enum LogIdError {
    /// Conversion to u64 from string failed.
    #[error("string contains invalid u64 value")]
    InvalidU64String,
}
