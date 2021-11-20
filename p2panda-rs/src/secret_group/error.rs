// SPDX-License-Identifier: AGPL-3.0-or-later

use thiserror::Error;

/// Custom error types for `SecretGroup`.
#[derive(Error, Debug)]
#[allow(missing_copy_implementations)]
pub enum SecretGroupError {
    #[error("Commit does not contain welcome message")]
    WelcomeMissing,

    #[error("MLS commit needs to be in plaintext")]
    NeedsToBeMlsPlaintext,

    #[error("MLS message is not a commit")]
    NeedsToBeMlsCommit,

    #[error(transparent)]
    LTSError(#[from] crate::secret_group::lts::LongTermSecretError),

    #[error("Long term secret has an invalid group id")]
    LTSInvalidGroupID,

    #[error("Can not decrypt long term secret since key material is missing")]
    LTSSecretMissing,

    #[error("Could not en- or decode long term secret")]
    LTSEncodingError,

    #[error(transparent)]
    MlsError(#[from] crate::secret_group::mls::MlsError),
}
