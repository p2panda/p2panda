// SPDX-License-Identifier: AGPL-3.0-or-later

use thiserror::Error;

/// Custom error types for `SecretGroup`.
#[derive(Error, Debug)]
#[allow(missing_copy_implementations)]
pub enum SecretGroupError {
    /// Commit messages and new long-term secrets can only be created by group owners.
    #[error("this method can only be used by group owners")]
    NotOwner,

    /// MLS commit message was expected to contain a welcome message as well.
    #[error("commit does not contain welcome message")]
    WelcomeMissing,

    /// Long-term secret does not match current secret group.
    #[error("long-term secret has an invalid group id")]
    LTSInvalidGroupID,

    /// User data could not be decrypted since long-term secret is missing.
    #[error("can not decrypt long-term secret since key material is missing")]
    LTSSecretMissing,

    /// LTS secret encoding failed.
    #[error("could not encode long-term secret")]
    LTSEncodingError,

    /// LTS secret decoding failed. Maybe the data was corrupted or invalid?
    #[error("could not decode long-term secret")]
    LTSDecodingError,

    /// Member's public key cannot be decoded as an Ed25519 public key.
    #[error("member's public key is not a valid Ed25519 public key")]
    InvalidMemberPublicKey,

    /// Decoding failed with unknown value.
    #[error("unknown value found during decoding")]
    UnknownValue,

    /// Error coming from `mls` sub-module.
    #[error(transparent)]
    MlsError(#[from] crate::next::secret_group::mls::error::MlsError),

    /// Error coming from `lts` sub-module.
    #[error(transparent)]
    LTSError(#[from] crate::next::secret_group::lts::error::LongTermSecretError),
}
