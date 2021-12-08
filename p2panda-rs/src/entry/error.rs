// SPDX-License-Identifier: AGPL-3.0-or-later

use thiserror::Error;

/// Error types for methods of `Entry` struct.
#[allow(missing_copy_implementations)]
#[derive(Error, Debug)]
pub enum EntryError {
    /// Links should not be set when first entry in log.
    #[error("backlink and skiplink not valid for this sequence number")]
    InvalidLinks,

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
    /// Encoded entry string contains invalid hex characters.
    #[error("invalid hex encoding in entry")]
    InvalidHexEncoding,

    /// Message needs to match payload hash of encoded entry
    #[error("message needs to match payload hash of encoded entry")]
    MessageHashMismatch,

    /// Can not sign and encode an entry without a `Message`.
    #[error("entry does not contain any message")]
    MessageMissing,

    /// Skiplink is required for entry encoding.
    #[error("entry requires skiplink for encoding")]
    SkiplinkMissing,

    /// Handle errors from `SeqNum` struct.
    #[error(transparent)]
    SeqNumError(#[from] SeqNumError),

    /// Handle errors from `Hash` struct.
    #[error(transparent)]
    HashError(#[from] crate::hash::HashError),

    /// Handle errors from `MessageEncoded` struct.
    #[error(transparent)]
    MessageEncodedError(#[from] crate::message::MessageEncodedError),

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
}
