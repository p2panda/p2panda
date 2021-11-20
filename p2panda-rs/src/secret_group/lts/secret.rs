// SPDX-License-Identifier: AGPL-3.0-or-later

use openmls::group::GroupId;
use serde::{Deserialize, Serialize};
use tls_codec::{Size, TlsByteVecU8, TlsDeserialize, TlsSerialize, TlsSize};

use crate::hash::Hash;
use crate::secret_group::aes;
use crate::secret_group::lts::{
    LongTermSecretCiphersuite, LongTermSecretCiphertext, LongTermSecretError,
};

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

#[derive(Debug, Clone, Serialize, Deserialize, TlsDeserialize, TlsSerialize, TlsSize)]
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

    pub(crate) fn group_id(&self) -> Result<Hash, LongTermSecretError> {
        Ok(Hash::new_from_bytes(self.group_id.as_slice().to_vec())?)
    }

    pub(crate) fn long_term_epoch(&self) -> LongTermSecretEpoch {
        self.long_term_epoch
    }

    pub(crate) fn encrypt(
        &self,
        data: &[u8],
    ) -> Result<LongTermSecretCiphertext, LongTermSecretError> {
        let (ciphertext, nonce) = match self.ciphersuite {
            LongTermSecretCiphersuite::PANDA_AES256GCMSIV => {
                aes::encrypt(self.value.as_slice(), data)?
            }
        };

        let long_term_secret = LongTermSecretCiphertext {
            group_id: self.group_id.clone(),
            long_term_epoch: self.long_term_epoch,
            ciphertext: ciphertext.into(),
            nonce: nonce.into(),
        };

        Ok(long_term_secret)
    }

    pub(crate) fn decrypt(
        &self,
        ciphertext: &LongTermSecretCiphertext,
    ) -> Result<Vec<u8>, LongTermSecretError> {
        if ciphertext.long_term_epoch != self.long_term_epoch {
            panic!("Epoch does not match");
        }

        if ciphertext.group_id != self.group_id {
            panic!("Group does not match");
        }

        let plaintext = match self.ciphersuite {
            LongTermSecretCiphersuite::PANDA_AES256GCMSIV => aes::decrypt(
                self.value.as_slice(),
                ciphertext.nonce.as_slice(),
                ciphertext.ciphertext.as_slice(),
            )?,
        };

        Ok(plaintext)
    }
}
