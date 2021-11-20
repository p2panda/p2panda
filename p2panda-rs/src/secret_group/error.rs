// SPDX-License-Identifier: AGPL-3.0-or-later

use thiserror::Error;

/// Custom error types for `SecretGroup`.
#[derive(Error, Debug)]
#[allow(missing_copy_implementations)]
pub enum SecretGroupError {
    /// MLS commit message was expected to contain a welcome message as well.
    #[error("Commit does not contain welcome message")]
    WelcomeMissing,

    /// MLS commit message was expected to be in plaintext.
    #[error("MLS commit needs to be in plaintext")]
    NeedsToBeMlsPlaintext,

    /// MLS commit message was expected.
    #[error("MLS message is not a commit")]
    NeedsToBeMlsCommit,

    /// Long term secret does not match current secret group.
    #[error("Long term secret has an invalid group id")]
    LTSInvalidGroupID,

    /// User data could not be decrypted since long term secret is missing.
    #[error("Can not decrypt long term secret since key material is missing")]
    LTSSecretMissing,

    /// LTS secret encoding failed.
    #[error("Could not encode long term secret")]
    LTSEncodingError,

    /// LTS secret decoding failed. Maybe the data was corrupted or invalid?
    #[error("Could not decode long term secret")]
    LTSDecodingError,

    /// Error coming from `mls` sub-module.
    #[error(transparent)]
    MlsError(#[from] crate::secret_group::mls::MlsError),

    /// Error coming from `lts` sub-module.
    #[error(transparent)]
    LTSError(#[from] crate::secret_group::lts::LongTermSecretError),
}
