// SPDX-License-Identifier: MIT OR Apache-2.0

//! Hybrid Public Key Encryption (HPKE) with DHKEM-X25519, HKDF SHA256 and AES-256-GCM AEAD
//! parameters.
//!
//! <https://www.rfc-editor.org/rfc/rfc9180>
// TODO: Switch to `libcrux-hpke` as soon as it's ready.
use libcrux::hpke::{HPKEConfig, HpkeOpen, HpkeSeal, Mode, aead, errors, kdf, kem};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::crypto::{PublicKey, SecretKey};
use crate::traits::RandProvider;

const KEM: kem::KEM = kem::KEM::DHKEM_X25519_HKDF_SHA256;
const KDF: kdf::KDF = kdf::KDF::HKDF_SHA256;
const AEAD: aead::AEAD = aead::AEAD::AES_256_GCM;

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
pub fn hpke_seal<RNG: RandProvider>(
    public_key: &PublicKey,
    info: Option<&[u8]>,
    aad: Option<&[u8]>,
    plaintext: &[u8],
    rng: &RNG,
) -> Result<HpkeCiphertext, HpkeError<RNG>> {
    let config = HPKEConfig(Mode::mode_base, KEM, KDF, AEAD);
    let randomness = rng
        .random_vec(kem::Nsk(KEM))
        .map_err(|err| HpkeError::Rand(err))?;
    let libcrux::hpke::HPKECiphertext(kem_output, ciphertext) = HpkeSeal(
        config,
        public_key.as_bytes(),
        info.unwrap_or_default(),
        aad.unwrap_or_default(),
        plaintext,
        None,
        None,
        None,
        randomness,
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
pub fn hpke_open<RNG: RandProvider>(
    input: &HpkeCiphertext,
    secret_key: &SecretKey,
    info: Option<&[u8]>,
    aad: Option<&[u8]>,
) -> Result<Vec<u8>, HpkeError<RNG>> {
    let config = HPKEConfig(Mode::mode_base, KEM, KDF, AEAD);
    let ciphertext =
        libcrux::hpke::HPKECiphertext(input.kem_output.to_vec(), input.ciphertext.to_vec());
    let plaintext = HpkeOpen(
        config,
        &ciphertext,
        secret_key.as_bytes(),
        info.unwrap_or_default(),
        aad.unwrap_or_default(),
        None,
        None,
        None,
    )
    .map_err(HpkeError::Decryption)?;
    Ok(plaintext)
}

#[derive(Debug, Error)]
pub enum HpkeError<RNG: RandProvider> {
    #[error(transparent)]
    Rand(RNG::Error),

    #[error("could not encrypt with hpke: {0:?}")]
    Encryption(errors::HpkeError),

    #[error("could not decrypt with hpke: {0:?}")]
    Decryption(errors::HpkeError),
}

#[cfg(test)]
mod tests {
    use crate::crypto::{Crypto, SecretKey};
    use crate::traits::RandProvider;

    use super::{HpkeError, hpke_open, hpke_seal};

    #[test]
    fn seal_and_open() {
        let rng = Crypto::from_seed([1; 32]);

        let secret_key = SecretKey::from_bytes(rng.random_array().unwrap());
        let public_key = secret_key.public_key().unwrap();

        let info = b"some info";
        let aad = b"some aad";
        let ciphertext =
            hpke_seal(&public_key, Some(info), Some(aad), b"Hello, Panda!", &rng).unwrap();
        let plaintext =
            hpke_open::<Crypto>(&ciphertext, &secret_key, Some(info), Some(aad)).unwrap();

        assert_eq!(plaintext, b"Hello, Panda!");
    }

    #[test]
    fn decryption_failed() {
        let rng = Crypto::from_seed([1; 32]);

        let valid_secret_key = SecretKey::from_bytes(rng.random_array().unwrap());
        let public_key = valid_secret_key.public_key().unwrap();

        let info = b"some info";
        let aad = b"some aad";
        let ciphertext =
            hpke_seal(&public_key, Some(info), Some(aad), b"Hello, Panda!", &rng).unwrap();

        // Invalid secret key.
        let invalid_secret_key = SecretKey::from_bytes(rng.random_array().unwrap());
        let result = hpke_open::<Crypto>(&ciphertext, &invalid_secret_key, Some(info), Some(aad));
        assert!(matches!(result, Err(HpkeError::Decryption(_))));

        // Invalid info tag.
        let result = hpke_open::<Crypto>(&ciphertext, &valid_secret_key, None, Some(aad));
        assert!(matches!(result, Err(HpkeError::Decryption(_))));

        // Invalid aad.
        let result = hpke_open::<Crypto>(&ciphertext, &valid_secret_key, Some(info), None);
        assert!(matches!(result, Err(HpkeError::Decryption(_))));
    }
}
