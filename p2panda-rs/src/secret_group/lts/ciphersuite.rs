// SPDX-License-Identifier: AGPL-3.0-or-later

use openmls_traits::types::AeadType;
use tls_codec::{TlsDeserialize, TlsSerialize, TlsSize};

/// List of currently supported ciphersuites for Long Term Secret encryption.
#[derive(Debug, Clone, PartialEq, Copy, TlsDeserialize, TlsSerialize, TlsSize)]
#[repr(u8)]
#[allow(non_camel_case_types)]
pub enum LongTermSecretCiphersuite {
    PANDA10_AES256GCM = 0x01,
}

impl LongTermSecretCiphersuite {
    /// Helper method to convert to internal MLS AEAD types for from LTS Ciphersuite.
    pub fn mls_aead_type(&self) -> AeadType {
        match self {
            Self::PANDA10_AES256GCM => AeadType::Aes256Gcm,
        }
    }
}
