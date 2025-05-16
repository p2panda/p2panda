// SPDX-License-Identifier: MIT OR Apache-2.0

//! Hybrid Public Key Encryption (HPKE) with DHKEM-X25519, HKDF SHA256 and ChaCha20Poly1305 AEAD
//! parameters.
//!
//! <https://www.rfc-editor.org/rfc/rfc9180>
use hpke_rs::{Hpke, HpkePrivateKey, HpkePublicKey, Mode};
use hpke_rs_crypto::types::{AeadAlgorithm, KdfAlgorithm, KemAlgorithm};
use hpke_rs_rust_crypto::HpkeRustCrypto;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::crypto::x25519::{PublicKey, SecretKey};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HpkeCiphertext {
    /// Encrypted, shared secret generated for this transaction.
    #[serde(with = "serde_bytes")]
    pub kem_output: Vec<u8>,

    /// Encrypted payload.
    #[serde(with = "serde_bytes")]
    pub ciphertext: Vec<u8>,
}

/// Encrypt a secret payload to a public key using HPKE.
///
/// The sender in HPKE uses a KEM to generate the shared secret as well as the encapsulation. The
/// shared secret is then used in an AEAD (after running it through a key schedule) in order to
/// encrypt a payload.
///
/// In order to encrypt a payload to a public key the sender needs to provide the receiver’s public
/// key, some information `info` and additional data `aad` to bind the encryption to a certain
/// context, as well as the payload `plaintext`.
pub fn hpke_seal(
    public_key: &PublicKey,
    info: Option<&[u8]>,
    aad: Option<&[u8]>,
    plaintext: &[u8],
) -> Result<HpkeCiphertext, HpkeError> {
    // Unfortunately `hpke-rs` doesn't allow us to pass in our own rng without writing a lot of
    // boilerplate, so we hope to replace it with a different API or solution sometime.
    let mut hpke = Hpke::<HpkeRustCrypto>::new(
        Mode::Base,
        KemAlgorithm::DhKem25519,
        KdfAlgorithm::HkdfSha256,
        AeadAlgorithm::ChaCha20Poly1305,
    );
    let pk_r = HpkePublicKey::new(public_key.as_bytes().to_vec());
    let (kem_output, ciphertext) = hpke
        .seal(
            &pk_r,
            info.unwrap_or_default(),
            aad.unwrap_or_default(),
            plaintext,
            None,
            None,
            None,
        )
        .map_err(HpkeError::Encryption)?;
    Ok(HpkeCiphertext {
        kem_output,
        ciphertext,
    })
}

/// Decrypt a secret payload for a receiver holding the secret key using HPKE.
///
/// When decrypting the receiver uses the secret key to retrieve the shared secret and decrypt the
/// ciphertext. The `info` and `aad` (additional data) are the same as entered on the sender’s
/// side.
pub fn hpke_open(
    input: &HpkeCiphertext,
    secret_key: &SecretKey,
    info: Option<&[u8]>,
    aad: Option<&[u8]>,
) -> Result<Vec<u8>, HpkeError> {
    let hpke = Hpke::<HpkeRustCrypto>::new(
        Mode::Base,
        KemAlgorithm::DhKem25519,
        KdfAlgorithm::HkdfSha256,
        AeadAlgorithm::ChaCha20Poly1305,
    );
    let sk_r = HpkePrivateKey::new(secret_key.as_bytes().to_vec());
    let plaintext = hpke
        .open(
            &input.kem_output,
            &sk_r,
            info.unwrap_or_default(),
            aad.unwrap_or_default(),
            &input.ciphertext,
            None,
            None,
            None,
        )
        .map_err(HpkeError::Decryption)?;
    Ok(plaintext)
}

#[derive(Debug, Error)]
pub enum HpkeError {
    #[error("could not encrypt with hpke: {0:?}")]
    Encryption(hpke_rs::HpkeError),

    #[error("could not decrypt with hpke: {0:?}")]
    Decryption(hpke_rs::HpkeError),
}

#[cfg(test)]
mod tests {
    use crate::crypto::Rng;
    use crate::crypto::x25519::SecretKey;

    use super::{HpkeError, hpke_open, hpke_seal};

    #[test]
    fn seal_and_open() {
        let rng = Rng::from_seed([1; 32]);

        let secret_key = SecretKey::from_bytes(rng.random_array().unwrap());
        let public_key = secret_key.public_key().unwrap();

        let info = b"some info";
        let aad = b"some aad";
        let ciphertext = hpke_seal(&public_key, Some(info), Some(aad), b"Hello, Panda!").unwrap();
        let plaintext = hpke_open(&ciphertext, &secret_key, Some(info), Some(aad)).unwrap();

        assert_eq!(plaintext, b"Hello, Panda!");
    }

    #[test]
    fn decryption_failed() {
        let rng = Rng::from_seed([1; 32]);

        let valid_secret_key = SecretKey::from_bytes(rng.random_array().unwrap());
        let public_key = valid_secret_key.public_key().unwrap();

        let info = b"some info";
        let aad = b"some aad";
        let ciphertext = hpke_seal(&public_key, Some(info), Some(aad), b"Hello, Panda!").unwrap();

        // Invalid secret key.
        let invalid_secret_key = SecretKey::from_bytes(rng.random_array().unwrap());
        let result = hpke_open(&ciphertext, &invalid_secret_key, Some(info), Some(aad));
        assert!(matches!(result, Err(HpkeError::Decryption(_))));

        // Invalid info tag.
        let result = hpke_open(&ciphertext, &valid_secret_key, None, Some(aad));
        assert!(matches!(result, Err(HpkeError::Decryption(_))));

        // Invalid aad.
        let result = hpke_open(&ciphertext, &valid_secret_key, Some(info), None);
        assert!(matches!(result, Err(HpkeError::Decryption(_))));
    }
}
