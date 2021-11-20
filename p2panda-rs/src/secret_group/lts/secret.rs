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

/// Long term secrets are objects which hold sensitive AEAD key secrets used to symmetrically
/// encrypt user data for longer periods of time.
///
/// Additionally to the secret value every long term secret also holds meta data, like the MLS
/// group id and epoch which this secret belongs to.
#[derive(Debug, Clone, Serialize, Deserialize, TlsDeserialize, TlsSerialize, TlsSize)]
pub struct LongTermSecret {
    /// Identifier of the related MLS group.
    group_id: GroupId,

    /// The corresponding ciphersuite for this secret.
    ciphersuite: LongTermSecretCiphersuite,

    /// Epoch of this long term secret.
    long_term_epoch: LongTermSecretEpoch,

    /// Symmetrical secret key used for AEAD encryption.
    value: TlsByteVecU8,
}

impl LongTermSecret {
    /// Creates a new instance of `LongTermSecret`.
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

    /// Returns the instance hash of the `SecretGroup` of this long term secret.
    ///
    /// This method can throw an error when the secret contains an invalid secret group instance
    /// hash.
    pub fn group_instance_id(&self) -> Result<Hash, LongTermSecretError> {
        let hex_str = hex::encode(&self.group_id.as_slice());
        Ok(Hash::new(&hex_str)?)
    }

    /// Returns the epoch of this long term secret.
    pub fn long_term_epoch(&self) -> LongTermSecretEpoch {
        self.long_term_epoch
    }

    /// Encrypts user data with the given secret and returns a ciphertext object holding the
    /// encrypted data and needed meta information like the nonce to decrypt it again.
    pub fn encrypt(
        &self,
        provider: &impl OpenMlsCryptoProvider,
        data: &[u8],
    ) -> Result<LongTermSecretCiphertext, LongTermSecretError> {
        // Decrypts data with secret key and receives ciphertext and used nonce
        let (ciphertext, nonce) = match self.ciphersuite {
            LongTermSecretCiphersuite::PANDA_AES256GCMSIV => {
                aes::encrypt(provider, self.value.as_slice(), data)?
            }
        };

        Ok(LongTermSecretCiphertext::new(
            self.group_instance_id()?,
            self.long_term_epoch(),
            ciphertext,
            nonce,
        ))
    }

    /// Decrypts a `LongTermSecretCiphertext` object with encrypted user data.
    pub fn decrypt(
        &self,
        ciphertext: &LongTermSecretCiphertext,
    ) -> Result<Vec<u8>, LongTermSecretError> {
        // The used secret does not match the ciphertexts epoch
        if ciphertext.long_term_epoch() != self.long_term_epoch {
            return Err(LongTermSecretError::EpochNotMatching);
        }

        // The used secret does not match the ciphertexts group instance hash
        if ciphertext.group_instance_id()? != self.group_instance_id()? {
            return Err(LongTermSecretError::GroupNotMatching);
        }

        let plaintext = match self.ciphersuite {
            LongTermSecretCiphersuite::PANDA_AES256GCMSIV => aes::decrypt(
                self.value.as_slice(),
                &ciphertext.nonce(),
                &ciphertext.ciphertext(),
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
