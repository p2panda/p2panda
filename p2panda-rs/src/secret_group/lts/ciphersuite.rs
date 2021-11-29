// SPDX-License-Identifier: AGPL-3.0-or-later

use openmls_traits::types::AeadType;
use tls_codec::{TlsDeserialize, TlsSerialize, TlsSize};

/// List of currently supported ciphersuites for Long Term Secret encryption.
#[derive(Debug, Clone, PartialEq, Copy, TlsDeserialize, TlsSerialize, TlsSize)]
#[repr(u8)]
#[allow(non_camel_case_types)]
pub enum LongTermSecretCiphersuite {
    /// AES128 GCM.
    PANDA10_AES128GCM = 0x01,

    /// AES256 GCM.
    PANDA10_AES256GCM = 0x02,

    /// ChaCha20Poly1305.
    PANDA10_CHACHA20POLY1305 = 0x03,
}

impl LongTermSecretCiphersuite {
    #[cfg(test)]
    pub fn ciphersuites() -> Vec<LongTermSecretCiphersuite> {
        vec![
            LongTermSecretCiphersuite::PANDA10_AES128GCM,
            LongTermSecretCiphersuite::PANDA10_AES256GCM,
            LongTermSecretCiphersuite::PANDA10_CHACHA20POLY1305,
        ]
    }

    /// Helper method to convert to internal MLS AEAD types for from LTS Ciphersuite.
    pub fn mls_aead_type(&self) -> AeadType {
        match self {
            Self::PANDA10_AES128GCM => AeadType::Aes128Gcm,
            Self::PANDA10_AES256GCM => AeadType::Aes256Gcm,
            Self::PANDA10_CHACHA20POLY1305 => AeadType::ChaCha20Poly1305,
        }
    }
}
