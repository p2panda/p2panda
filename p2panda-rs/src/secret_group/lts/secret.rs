// SPDX-License-Identifier: AGPL-3.0-or-later

use openmls::group::GroupId;
use openmls_traits::OpenMlsCryptoProvider;
use serde::{Deserialize, Serialize};
use tls_codec::{Size, TlsByteVecU8, TlsDeserialize, TlsSerialize, TlsSize};

use crate::hash::Hash;
use crate::secret_group::aes;
use crate::secret_group::lts::{
    LongTermSecretCiphersuite, LongTermSecretCiphertext, LongTermSecretEpoch, LongTermSecretError,
};

#[derive(Debug, Clone, Serialize, Deserialize, TlsDeserialize, TlsSerialize, TlsSize)]
pub struct LongTermSecret {
    group_id: GroupId,
    ciphersuite: LongTermSecretCiphersuite,
    long_term_epoch: LongTermSecretEpoch,
    value: TlsByteVecU8,
}

impl LongTermSecret {
    pub fn new(
        group_instance_id: Hash,
        ciphersuite: LongTermSecretCiphersuite,
        long_term_epoch: LongTermSecretEpoch,
        value: TlsByteVecU8,
    ) -> Self {
        Self {
            // Convert group instance id Hash to internal MLS GroupId struct which implements
            // required TLS encoding traits
            group_id: GroupId::from_slice(&group_instance_id.to_bytes()),
            ciphersuite,
            long_term_epoch,
            value,
        }
    }

    /// This method can throw an error when the secret contains an invalid secret group instance
    /// hash.
    pub fn group_instance_id(&self) -> Result<Hash, LongTermSecretError> {
        let hex_str = hex::encode(&self.group_id.as_slice());
        Ok(Hash::new(&hex_str)?)
    }

    pub fn long_term_epoch(&self) -> LongTermSecretEpoch {
        self.long_term_epoch
    }

    pub fn encrypt(
        &self,
        provider: &impl OpenMlsCryptoProvider,
        data: &[u8],
    ) -> Result<LongTermSecretCiphertext, LongTermSecretError> {
        let (ciphertext, nonce) = match self.ciphersuite {
            LongTermSecretCiphersuite::PANDA_AES256GCMSIV => {
                aes::encrypt(provider, self.value.as_slice(), data)?
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

    pub fn decrypt(
        &self,
        ciphertext: &LongTermSecretCiphertext,
    ) -> Result<Vec<u8>, LongTermSecretError> {
        if ciphertext.long_term_epoch != self.long_term_epoch {
            return Err(LongTermSecretError::EpochNotMatching);
        }

        if ciphertext.group_id != self.group_id {
            return Err(LongTermSecretError::GroupNotMatching);
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

#[cfg(test)]
mod tests {
    use crate::hash::Hash;
    use crate::secret_group::lts::{LongTermSecretCiphersuite, LongTermSecretEpoch};

    use super::LongTermSecret;

    #[test]
    fn group_id_hash_encoding() {
        let group_instance_id = Hash::new_from_bytes(vec![1, 2, 3]).unwrap();

        let secret = LongTermSecret::new(
            group_instance_id.clone(),
            LongTermSecretCiphersuite::PANDA_AES256GCMSIV,
            LongTermSecretEpoch(0),
            vec![1, 2, 3].into(),
        );

        // Make sure the conversion between p2panda `Hash` and MLS `GroupId` works
        assert_eq!(
            group_instance_id.as_str(),
            secret.group_instance_id().unwrap().as_str()
        );
    }
}
