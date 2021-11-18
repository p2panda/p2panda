// SPDX-License-Identifier: AGPL-3.0-or-later

use serde::{Deserialize, Serialize};
use tls_codec::{Size, TlsByteVecU8, TlsDeserialize, TlsSerialize, TlsSize};

use crate::secret_group::lts::LongTermSecretCiphersuite;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TlsDeserialize, TlsSerialize, TlsSize)]
pub struct LongTermSecretEpoch(pub u64);

impl LongTermSecretEpoch {
    pub fn increment(&mut self) {
        self.0 += 1;
    }
}

#[derive(Debug, Serialize, Deserialize, TlsDeserialize, TlsSerialize, TlsSize)]
pub struct LongTermSecret {
    ciphersuite: LongTermSecretCiphersuite,
    long_term_epoch: LongTermSecretEpoch,
    value: TlsByteVecU8,
}

impl LongTermSecret {
    pub fn new(
        ciphersuite: LongTermSecretCiphersuite,
        long_term_epoch: LongTermSecretEpoch,
        value: TlsByteVecU8,
    ) -> Self {
        Self {
            ciphersuite,
            long_term_epoch,
            value,
        }
    }
}
