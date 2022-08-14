// SPDX-License-Identifier: AGPL-3.0-or-later

use openmls_traits::types::AeadType;
use tls_codec::{TlsDeserialize, TlsSerialize, TlsSize};

/// List of currently supported ciphersuites for Long Term Secret encryption.
#[derive(Debug, Clone, Eq, PartialEq, Copy, TlsDeserialize, TlsSerialize, TlsSize)]
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

impl Default for LongTermSecretCiphersuite {
    fn default() -> Self {
        Self::PANDA10_AES256GCM
    }
}

impl LongTermSecretCiphersuite {
    /// Returns a list of all supported ciphersuites in this implementation.
    pub fn supported_ciphersuites() -> Vec<LongTermSecretCiphersuite> {
        vec![
            LongTermSecretCiphersuite::PANDA10_AES128GCM,
            LongTermSecretCiphersuite::PANDA10_AES256GCM,
            LongTermSecretCiphersuite::PANDA10_CHACHA20POLY1305,
        ]
    }

    /// Return AEAD key length.
    pub fn aead_key_length(&self) -> usize {
        match self {
            LongTermSecretCiphersuite::PANDA10_AES128GCM => 16,
            LongTermSecretCiphersuite::PANDA10_AES256GCM => 32,
            LongTermSecretCiphersuite::PANDA10_CHACHA20POLY1305 => 32,
        }
    }

    /// Return AEAD nonce length.
    pub fn aead_nonce_length(&self) -> usize {
        match self {
            LongTermSecretCiphersuite::PANDA10_AES128GCM
            | LongTermSecretCiphersuite::PANDA10_AES256GCM
            | LongTermSecretCiphersuite::PANDA10_CHACHA20POLY1305 => 12,
        }
    }

    /// Internal method to return MLS `AeadType`
    pub(crate) fn mls_aead_type(&self) -> AeadType {
        match self {
            LongTermSecretCiphersuite::PANDA10_AES128GCM => AeadType::Aes128Gcm,
            LongTermSecretCiphersuite::PANDA10_AES256GCM => AeadType::Aes256Gcm,
            LongTermSecretCiphersuite::PANDA10_CHACHA20POLY1305 => AeadType::ChaCha20Poly1305,
        }
    }
}
