// SPDX-License-Identifier: MIT OR Apache-2.0

//! Extended Triple Diffie-Hellman (X3DH) key agreement protocol as specified by Signal.
//!
//! X3DH establishes a shared secret key between two parties who mutually authenticate each other
//! based on public keys. X3DH provides forward secrecy and cryptographic deniability.
//!
//! X3DH is designed for asynchronous settings where one user ("Bob") is offline but has published
//! some information to a server. Another user ("Alice") wants to use that information to send
//! encrypted data to Bob, and also establish a shared secret key for future communication.
//!
//! <https://signal.org/docs/specifications/x3dh/>
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::crypto::aead::{AeadError, AeadNonce, aead_decrypt, aead_encrypt};
use crate::crypto::hkdf::{HkdfError, hkdf};
use crate::crypto::x25519::{PublicKey, SecretKey, X25519Error};
use crate::crypto::{Rng, RngError};
use crate::key_bundle::{KeyBundleError, OneTimeKeyId};
use crate::traits::KeyBundle;

/// ASCII string identifying the application as specified in X3DH used for KDF.
const KDF_INFO: &[u8; 7] = b"p2panda";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreKeyCiphertext {
    pub identity_key: PublicKey,
    pub onetime_prekey_id: Option<OneTimeKeyId>,
    pub ciphertext: Vec<u8>,
    pub ephemeral_key: PublicKey,
}

pub fn x3dh_encrypt<K: KeyBundle>(
    plaintext: &[u8],
    our_identity_secret: &SecretKey,
    their_prekey_bundle: &K,
    rng: &Rng,
) -> Result<PreKeyCiphertext, X3DHError> {
    their_prekey_bundle.verify()?;

    let our_identity_key = our_identity_secret.public_key()?;

    let our_ephemeral_secret = SecretKey::from_bytes(rng.random_array()?);
    let our_ephemeral_key = our_ephemeral_secret.public_key()?;

    let mut ikm = Vec::with_capacity({
        if their_prekey_bundle.onetime_prekey().is_none() {
            32 * 4
        } else {
            32 * 5
        }
    });

    ikm.extend_from_slice(&[0xFFu8; 32]); // "discontinuity bytes"

    ikm.extend_from_slice(
        &our_identity_secret.calculate_agreement(their_prekey_bundle.signed_prekey())?,
    );

    ikm.extend_from_slice(
        &our_ephemeral_secret.calculate_agreement(their_prekey_bundle.identity_key())?,
    );

    ikm.extend_from_slice(
        &our_ephemeral_secret.calculate_agreement(their_prekey_bundle.signed_prekey())?,
    );

    if let Some(onetime_prekey) = their_prekey_bundle.onetime_prekey() {
        ikm.extend_from_slice(&our_ephemeral_secret.calculate_agreement(onetime_prekey)?);
    }

    let sk: [u8; 32] = {
        let salt = vec![0_u8; 32];
        hkdf(&salt, &ikm, Some(KDF_INFO))?
    };

    drop(our_ephemeral_secret);
    drop(ikm);

    let ad = {
        let mut buf = Vec::new();
        buf.extend_from_slice(our_identity_key.as_bytes());
        buf.extend_from_slice(their_prekey_bundle.identity_key().as_bytes());
        buf
    };

    let nonce: AeadNonce = hkdf(b"", &sk, None)?;
    let ciphertext = aead_encrypt(&sk, plaintext, nonce, Some(&ad))?;

    Ok(PreKeyCiphertext {
        ciphertext,
        ephemeral_key: our_ephemeral_key,
        identity_key: our_identity_key,
        onetime_prekey_id: their_prekey_bundle.onetime_prekey_id(),
    })
}

pub fn x3dh_decrypt(
    their_ciphertext: &PreKeyCiphertext,
    our_identity_secret: &SecretKey,
    our_prekey_secret: &SecretKey,
    our_onetime_secret: Option<&SecretKey>,
) -> Result<Vec<u8>, X3DHError> {
    let our_identity_key = our_identity_secret.public_key()?;

    let mut ikm = Vec::with_capacity(if our_onetime_secret.is_none() {
        32 * 4
    } else {
        32 * 5
    });

    ikm.extend_from_slice(&[0xFFu8; 32]); // "discontinuity bytes"

    // DH1 = DH(IKA, SPKB)
    ikm.extend_from_slice(&our_prekey_secret.calculate_agreement(&their_ciphertext.identity_key)?);

    // DH2 = DH(EKA, IKB)
    ikm.extend_from_slice(
        &our_identity_secret.calculate_agreement(&their_ciphertext.ephemeral_key)?,
    );

    // DH3 = DH(EKA, SPKB)
    ikm.extend_from_slice(&our_prekey_secret.calculate_agreement(&their_ciphertext.ephemeral_key)?);

    // DH4 = DH(EKA, OPKB)
    if let Some(our_onetime_secret) = our_onetime_secret {
        ikm.extend_from_slice(
            &our_onetime_secret.calculate_agreement(&their_ciphertext.ephemeral_key)?,
        );
    }

    let sk: [u8; 32] = {
        let salt = vec![0_u8; 32];
        hkdf(&salt, &ikm, Some(KDF_INFO))?
    };

    drop(ikm);

    let ad = {
        let mut buf = Vec::new();
        buf.extend_from_slice(their_ciphertext.identity_key.as_bytes());
        buf.extend_from_slice(our_identity_key.as_bytes());
        buf
    };

    let nonce: AeadNonce = hkdf(b"", &sk, None)?;
    let plaintext = aead_decrypt(&sk, &their_ciphertext.ciphertext, nonce, Some(&ad))?;

    Ok(plaintext)
}

#[derive(Debug, Error)]
pub enum X3DHError {
    #[error(transparent)]
    Rng(#[from] RngError),

    #[error(transparent)]
    Aead(#[from] AeadError),

    #[error(transparent)]
    Hkdf(#[from] HkdfError),

    #[error(transparent)]
    X25519(#[from] X25519Error),

    #[error(transparent)]
    KeyBundle(#[from] KeyBundleError),
}

#[cfg(test)]
mod tests {
    use crate::crypto::Rng;
    use crate::crypto::x25519::SecretKey;
    use crate::key_bundle::{Lifetime, LongTermKeyBundle, OneTimeKey, OneTimeKeyBundle, PreKey};

    use super::{x3dh_decrypt, x3dh_encrypt};

    #[test]
    fn encrypt_decrypt() {
        let rng = Rng::from_seed([1; 32]);

        let bob_identity_secret = SecretKey::from_bytes(rng.random_array().unwrap());

        let bob_prekey_secret = SecretKey::from_bytes(rng.random_array().unwrap());
        let bob_signed_prekey =
            PreKey::new(bob_prekey_secret.public_key().unwrap(), Lifetime::default());

        let bob_onetime_secret = SecretKey::from_bytes(rng.random_array().unwrap());
        let bob_onetime_prekey = OneTimeKey::new(bob_onetime_secret.public_key().unwrap(), 2);

        let bob_prekey_signature = bob_signed_prekey.sign(&bob_identity_secret, &rng).unwrap();

        let bob_prekey_bundle = OneTimeKeyBundle::new(
            bob_identity_secret.public_key().unwrap(),
            bob_signed_prekey,
            bob_prekey_signature,
            Some(bob_onetime_prekey),
        );

        let alice_identity_secret = SecretKey::from_bytes(rng.random_array().unwrap());

        let ciphertext = x3dh_encrypt(
            b"Hello, Panda!",
            &alice_identity_secret,
            &bob_prekey_bundle,
            &rng,
        )
        .unwrap();

        let plaintext = x3dh_decrypt(
            &ciphertext,
            &bob_identity_secret,
            &bob_prekey_secret,
            Some(&bob_onetime_secret),
        )
        .unwrap();

        assert_eq!(b"Hello, Panda!", plaintext.as_slice());
    }

    #[test]
    fn longterm_key_bundle() {
        let rng = Rng::from_seed([1; 32]);

        let bob_identity_secret = SecretKey::from_bytes(rng.random_array().unwrap());

        let bob_prekey_secret = SecretKey::from_bytes(rng.random_array().unwrap());
        let bob_signed_prekey =
            PreKey::new(bob_prekey_secret.public_key().unwrap(), Lifetime::default());

        let bob_prekey_signature = bob_signed_prekey.sign(&bob_identity_secret, &rng).unwrap();

        let bob_prekey_bundle = LongTermKeyBundle::new(
            bob_identity_secret.public_key().unwrap(),
            bob_signed_prekey,
            bob_prekey_signature,
        );

        let alice_identity_secret = SecretKey::from_bytes(rng.random_array().unwrap());

        let ciphertext = x3dh_encrypt(
            b"Hello, Panda!",
            &alice_identity_secret,
            &bob_prekey_bundle,
            &rng,
        )
        .unwrap();

        let plaintext =
            x3dh_decrypt(&ciphertext, &bob_identity_secret, &bob_prekey_secret, None).unwrap();

        assert_eq!(b"Hello, Panda!", plaintext.as_slice());
    }
}
