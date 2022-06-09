// SPDX-License-Identifier: AGPL-3.0-or-later

use openmls_traits::crypto::OpenMlsCrypto;
use openmls_traits::OpenMlsCryptoProvider;

use crate::secret_group::lts::{LongTermSecretCiphersuite, LongTermSecretError};

/// Encrypt data using the AEAD algorithm given by ciphersuite. Returns ciphertext with
/// authenticated plaintext HMAC (tag).
pub fn encrypt(
    provider: &impl OpenMlsCryptoProvider,
    ciphersuite: &LongTermSecretCiphersuite,
    value: &[u8],
    data: &[u8],
    nonce: &[u8],
    aad: &[u8],
) -> Result<Vec<u8>, LongTermSecretError> {
    let ciphertext_tag =
        match ciphersuite {
            LongTermSecretCiphersuite::PANDA10_AES128GCM
            | LongTermSecretCiphersuite::PANDA10_AES256GCM
            | LongTermSecretCiphersuite::PANDA10_CHACHA20POLY1305 => provider
                .crypto()
                .aead_encrypt(ciphersuite.mls_aead_type(), value, data, nonce, aad)?,
        };

    Ok(ciphertext_tag)
}

/// Decrypt tagged ciphertext using the AEAD algorithm given by ciphersuite.
pub fn decrypt(
    provider: &impl OpenMlsCryptoProvider,
    ciphersuite: &LongTermSecretCiphersuite,
    value: &[u8],
    ciphertext_tag: &[u8],
    nonce: &[u8],
    aad: &[u8],
) -> Result<Vec<u8>, LongTermSecretError> {
    let plaintext = match ciphersuite {
        LongTermSecretCiphersuite::PANDA10_AES128GCM
        | LongTermSecretCiphersuite::PANDA10_AES256GCM
        | LongTermSecretCiphersuite::PANDA10_CHACHA20POLY1305 => provider.crypto().aead_decrypt(
            ciphersuite.mls_aead_type(),
            value,
            ciphertext_tag,
            nonce,
            aad,
        )?,
    };

    Ok(plaintext)
}
