// SPDX-License-Identifier: AGPL-3.0-or-later

use openmls::framing::{
    MlsMessageIn, MlsMessageOut, MlsPlaintext, MlsPlaintextContentType, VerifiableMlsPlaintext,
};
use openmls::messages::Welcome;

use crate::secret_group::SecretGroupMessage;

#[derive(Debug)]
pub struct SecretGroupCommit {
    mls_commit_message: MlsPlaintext,
    mls_welcome_message: Option<Welcome>,
    encrypted_long_term_secrets: SecretGroupMessage,
}

impl SecretGroupCommit {
    pub fn new(
        mls_message_out: MlsMessageOut,
        mls_welcome_message: Option<Welcome>,
        encrypted_long_term_secrets: SecretGroupMessage,
    ) -> Self {
        // Check if message is in plaintext
        let mls_commit_message = match mls_message_out {
            MlsMessageOut::Plaintext(message) => message,
            _ => panic!("This should never happen"),
        };

        // Check if message is a commit
        if match mls_commit_message.content() {
            MlsPlaintextContentType::Commit(..) => false,
            _ => true,
        } {
            panic!("This should never happen")
        }

        Self {
            mls_commit_message,
            mls_welcome_message,
            encrypted_long_term_secrets,
        }
    }

    pub fn commit(&self) -> MlsMessageIn {
        let message_clone = self.mls_commit_message.clone();
        MlsMessageIn::Plaintext(VerifiableMlsPlaintext::from_plaintext(message_clone, None))
    }

    pub fn welcome(&self) -> Option<Welcome> {
        self.mls_welcome_message.clone()
    }

    pub fn long_term_secret(&self) -> SecretGroupMessage {
        self.encrypted_long_term_secrets.clone()
    }
}
