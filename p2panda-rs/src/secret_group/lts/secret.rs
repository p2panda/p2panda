// SPDX-License-Identifier: AGPL-3.0-or-later

use openmls::group::GroupId;
use openmls_traits::crypto::OpenMlsCrypto;
use openmls_traits::OpenMlsCryptoProvider;
use tls_codec::{TlsByteVecU8, TlsDeserialize, TlsSerialize, TlsSize};

use crate::hash::Hash;
use crate::secret_group::lts::{
    LongTermSecretCiphersuite, LongTermSecretCiphertext, LongTermSecretEpoch, LongTermSecretError,
};

/// Long term secrets are objects which hold sensitive AEAD key secrets used to symmetrically
/// encrypt user data for longer periods of time.
///
/// Additionally to the secret value every long term secret also holds meta data, like the MLS
/// group id and epoch which this secret belongs to.
#[derive(Debug, Clone, PartialEq, TlsDeserialize, TlsSerialize, TlsSize)]
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
        nonce: &[u8],
        data: &[u8],
    ) -> Result<LongTermSecretCiphertext, LongTermSecretError> {
        // Decrypts data with secret key and receive ciphertext plus AAD tag
        let ciphertext_tag = provider.crypto().aead_encrypt(
            self.ciphersuite.mls_aead_type(),
            self.value.as_slice(),
            data,
            nonce,
            // Use group id as AAD
            self.group_id.as_slice(),
        )?;

        Ok(LongTermSecretCiphertext::new(
            self.group_instance_id()?,
            self.long_term_epoch(),
            ciphertext_tag,
            nonce.to_vec(),
        ))
    }

    /// Decrypts a `LongTermSecretCiphertext` object with encrypted user data.
    pub fn decrypt(
        &self,
        provider: &impl OpenMlsCryptoProvider,
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

        // Decrypt ciphertext with tag and check AAD
        // @TODO: This is currently broken upstream
        // See: https://github.com/openmls/openmls/pull/587
        let payload = provider.crypto().aead_decrypt(
            self.ciphersuite.mls_aead_type(),
            self.value.as_slice(),
            &ciphertext.ciphertext_with_tag(),
            &ciphertext.nonce(),
            // Use group id as AAD
            self.group_id.as_slice(),
        )?;

        Ok(payload)
    }
}

#[cfg(test)]
mod tests {
    use openmls_traits::random::OpenMlsRand;
    use openmls_traits::OpenMlsCryptoProvider;

    use crate::hash::Hash;
    use crate::secret_group::lts::{
        LongTermSecret, LongTermSecretCiphersuite, LongTermSecretEpoch, LongTermSecretError,
    };
    use crate::secret_group::MlsProvider;

    #[test]
    fn group_id_hash_encoding() {
        let group_instance_id = Hash::new_from_bytes(vec![1, 2, 3]).unwrap();

        let secret = LongTermSecret::new(
            group_instance_id.clone(),
            LongTermSecretCiphersuite::PANDA10_AES256GCM,
            LongTermSecretEpoch(0),
            vec![1, 2, 3].into(),
        );

        // Make sure the conversion between p2panda `Hash` and MLS `GroupId` works
        assert_eq!(
            group_instance_id.as_str(),
            secret.group_instance_id().unwrap().as_str()
        );
    }

    #[test]
    fn invalid_ciphertext() {
        let provider = MlsProvider::new();

        for ciphersuite in LongTermSecretCiphersuite::ciphersuites() {
            let aead_key = provider
                .rand()
                .random_vec(ciphersuite.mls_aead_type().key_size())
                .unwrap();

            let group_instance_id = Hash::new_from_bytes(vec![1, 2, 3]).unwrap();
            let group_instance_id_2 = Hash::new_from_bytes(vec![4, 5, 6]).unwrap();

            let secret = LongTermSecret::new(
                group_instance_id.clone(),
                ciphersuite,
                LongTermSecretEpoch(0),
                aead_key.clone().into(),
            );

            let secret_different_group = LongTermSecret::new(
                group_instance_id_2,
                ciphersuite,
                LongTermSecretEpoch(0),
                aead_key.clone().into(),
            );

            let secret_different_epoch = LongTermSecret::new(
                group_instance_id,
                ciphersuite,
                LongTermSecretEpoch(2),
                aead_key.into(),
            );

            let aead_nonce = provider
                .rand()
                .random_vec(ciphersuite.mls_aead_type().nonce_size())
                .unwrap();
            let ciphertext = secret
                .encrypt(&provider, &aead_nonce, b"Secret Message")
                .unwrap();
            assert!(secret.decrypt(&provider, &ciphertext).is_ok());

            assert!(matches!(
                secret_different_epoch.decrypt(&provider, &ciphertext),
                Err(LongTermSecretError::EpochNotMatching)
            ));
            assert!(matches!(
                secret_different_group.decrypt(&provider, &ciphertext),
                Err(LongTermSecretError::GroupNotMatching)
            ));
        }
    }
}
