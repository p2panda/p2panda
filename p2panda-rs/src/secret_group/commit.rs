// SPDX-License-Identifier: AGPL-3.0-or-later

use openmls::framing::{
    MlsMessageIn, MlsMessageOut, MlsPlaintextContentType, VerifiableMlsPlaintext,
};
use openmls::messages::Welcome;
use tls_codec::{TlsDeserialize, TlsSerialize, TlsSize};

use crate::secret_group::{SecretGroupError, SecretGroupMessage};

/// Plaintext commit message which is published on the network to announce group changes.
///
/// A `SecretGroupCommit` always contains an MLS Commit message and the current, encrypted
/// long-term secrets for this group. Optionally it contains another MLS Welcome message in case
/// this commit invites new members into the group.
#[derive(Debug, Clone, TlsSerialize, TlsDeserialize, TlsSize)]
pub struct SecretGroupCommit {
    mls_commit_message: VerifiableMlsPlaintext,
    mls_welcome_message: Option<Welcome>,
    encrypted_long_term_secrets: SecretGroupMessage,
}

impl SecretGroupCommit {
    /// Returns a new instance of a [SecretGroupCommit] message.
    pub(crate) fn new(
        mls_message_out: MlsMessageOut,
        mls_welcome_message: Option<Welcome>,
        encrypted_long_term_secrets: SecretGroupMessage,
    ) -> Result<Self, SecretGroupError> {
        // Check if message is in plaintext
        // @TODO: This should be handled internally by `openmls` instead:
        // https://github.com/openmls/openmls/issues/584
        let mls_commit_message = match mls_message_out.clone() {
            MlsMessageOut::Plaintext(message) => Ok(message),
            _ => Err(SecretGroupError::NeedsToBeMlsPlaintext),
        }?;

        // Check if message is a commit
        if match mls_commit_message.content() {
            MlsPlaintextContentType::Commit(..) => false,
            _ => true,
        } {
            return Err(SecretGroupError::NeedsToBeMlsCommit);
        }

        Ok(Self {
            mls_commit_message: VerifiableMlsPlaintext::from_plaintext(mls_commit_message, None),
            mls_welcome_message,
            encrypted_long_term_secrets,
        })
    }

    /// Returns the MLS Commit message.
    pub(crate) fn commit(&self) -> MlsMessageIn {
        MlsMessageIn::Plaintext(self.mls_commit_message.clone())
    }

    /// Returns an MLS Welcome message when given.
    pub(crate) fn welcome(&self) -> Option<Welcome> {
        self.mls_welcome_message.clone()
    }

    /// Returns the encrypted and encoded long-term secrets.
    pub(crate) fn long_term_secrets(&self) -> SecretGroupMessage {
        self.encrypted_long_term_secrets.clone()
    }
}
