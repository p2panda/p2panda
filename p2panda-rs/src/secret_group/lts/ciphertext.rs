// SPDX-License-Identifier: AGPL-3.0-or-later

use openmls::group::GroupId;
use serde::{Deserialize, Serialize};
use tls_codec::{Size, TlsByteVecU8, TlsDeserialize, TlsSerialize, TlsSize};

use crate::hash::Hash;
use crate::secret_group::lts::{LongTermSecretEpoch, LongTermSecretError};

/// Data type holding encrypted application data from a sender with needed meta information for a
/// receiver to decrypt it again.
#[derive(Debug, Clone, Serialize, Deserialize, TlsDeserialize, TlsSerialize, TlsSize)]
pub struct LongTermSecretCiphertext {
    /// Identifier of the related MLS group.
    group_id: GroupId,

    /// Epoch of the long term secret which was used to encrypt data.
    long_term_epoch: LongTermSecretEpoch,

    /// Used nonce during AES encryption.
    nonce: TlsByteVecU8,

    /// Encrypted user data.
    ciphertext: TlsByteVecU8,
}

impl LongTermSecretCiphertext {
    /// Creates a new `LongTermSecretCiphertext` instance.
    pub fn new(
        group_instance_id: Hash,
        long_term_epoch: LongTermSecretEpoch,
        ciphertext: Vec<u8>,
        nonce: Vec<u8>,
    ) -> Self {
        Self {
            // Convert group instance id Hash to internal MLS GroupId struct which implements
            // required TLS encoding traits
            group_id: GroupId::from_slice(&group_instance_id.to_bytes()),
            long_term_epoch,
            nonce: nonce.into(),
            ciphertext: ciphertext.into(),
        }
    }

    /// This method can throw an error when the secret contains an invalid secret group instance
    /// hash.
    pub fn group_instance_id(&self) -> Result<Hash, LongTermSecretError> {
        let hex_str = hex::encode(&self.group_id.as_slice());
        Ok(Hash::new(&hex_str)?)
    }

    /// Returns epoch of long term secret used when data was encrypted.
    pub fn long_term_epoch(&self) -> LongTermSecretEpoch {
        self.long_term_epoch.clone()
    }

    /// Returns AES nonce when data was encrypted.
    pub fn nonce(&self) -> Vec<u8> {
        self.nonce.as_slice().to_vec()
    }

    /// Returns encrypted user data.
    pub fn ciphertext(&self) -> Vec<u8> {
        self.ciphertext.as_slice().to_vec()
    }
}
