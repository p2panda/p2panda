// SPDX-License-Identifier: AGPL-3.0-or-later

use openmls::group::GroupId;
use serde::{Deserialize, Serialize};
use tls_codec::{Size, TlsByteVecU8, TlsDeserialize, TlsSerialize, TlsSize};

use crate::secret_group::aes;
use crate::secret_group::lts::{LongTermSecretCiphersuite, LongTermSecretCiphertext};

#[derive(
    Debug,
    Clone,
    Copy,
    Eq,
    PartialEq,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    TlsDeserialize,
    TlsSerialize,
    TlsSize,
)]
pub struct LongTermSecretEpoch(pub u64);

impl LongTermSecretEpoch {
    pub fn increment(&mut self) {
        self.0 += 1;
    }
}

#[derive(Debug, Serialize, Deserialize, TlsDeserialize, TlsSerialize, TlsSize)]
pub struct LongTermSecret {
    group_id: GroupId,
    ciphersuite: LongTermSecretCiphersuite,
    long_term_epoch: LongTermSecretEpoch,
    value: TlsByteVecU8,
}

impl LongTermSecret {
    pub fn new(
        group_id: GroupId,
        ciphersuite: LongTermSecretCiphersuite,
        long_term_epoch: LongTermSecretEpoch,
        value: TlsByteVecU8,
    ) -> Self {
        Self {
            group_id,
            ciphersuite,
            long_term_epoch,
            value,
        }
    }

    pub(crate) fn long_term_epoch(&self) -> LongTermSecretEpoch {
        self.long_term_epoch
    }

    pub(crate) fn encrypt(&self, data: &[u8]) -> LongTermSecretCiphertext {
        let (ciphertext, nonce) = match self.ciphersuite {
            LongTermSecretCiphersuite::PANDA_AES256GCMSIV => {
                aes::encrypt(self.value.as_slice(), data).unwrap()
            }
        };

        LongTermSecretCiphertext {
            group_id: self.group_id.clone(),
            long_term_epoch: self.long_term_epoch,
            ciphertext: ciphertext.into(),
            nonce: nonce.into(),
        }
    }

    pub(crate) fn decrypt(&self, ciphertext: &LongTermSecretCiphertext) -> Vec<u8> {
        if ciphertext.long_term_epoch != self.long_term_epoch {
            panic!("Epoch does not match");
        }

        if ciphertext.group_id != self.group_id {
            panic!("Group does not match");
        }

        match self.ciphersuite {
            LongTermSecretCiphersuite::PANDA_AES256GCMSIV => aes::decrypt(
                self.value.as_slice(),
                ciphertext.nonce.as_slice(),
                ciphertext.ciphertext.as_slice(),
            )
            .unwrap(),
        }
    }
}
