// SPDX-License-Identifier: AGPL-3.0-or-later

use openmls::messages::Welcome;
use openmls::framing::{MlsPlaintext, VerifiableMlsPlaintext};

pub struct SecretGroupCommit {
    mls_commit_message: MlsPlaintext,
    mls_welcome_message: Option<Welcome>,
}

impl SecretGroupCommit {
    pub fn new(commit: MlsPlaintext, welcome: Option<Welcome>) -> Self {
        Self {
            mls_commit_message: commit,
            mls_welcome_message: welcome,
        }
    }

    pub fn commit(&self) -> VerifiableMlsPlaintext {
        let message_clone = self.mls_commit_message.clone();
        VerifiableMlsPlaintext::from_plaintext(message_clone, None)
    }

    pub fn welcome(&self) -> Option<Welcome> {
        self.mls_welcome_message.clone()
    }
}
