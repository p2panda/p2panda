// SPDX-License-Identifier: AGPL-3.0-or-later

/// Nonce used for AEAD encryption with Long Term Secrets.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct LongTermSecretNonce {
    counter: u64,
}

impl Default for LongTermSecretNonce {
    fn default() -> Self {
        Self { counter: 0 }
    }
}

impl LongTermSecretNonce {
    /// Increments and returns the nonce automatically.
    pub fn increment(&mut self) -> u64 {
        self.counter += 1;
        self.counter
    }
}
