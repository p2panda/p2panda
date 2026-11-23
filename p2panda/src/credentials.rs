// SPDX-License-Identifier: MIT OR Apache-2.0

use std::sync::Arc;

use p2panda_core::identity::{SIGNING_KEY_LEN, Signer};
use p2panda_core::{SigningKey, VerifyingKey};
use rand::Rng;
use rand::rand_core::UnwrapErr;
use rand::rngs::SysRng;
use serde::{Deserialize, Serialize};

// Re-export useful types.
pub use p2panda_encryption::crypto::x25519::{PublicKey, SECRET_KEY_SIZE, SecretKey, X25519Error};

/// Holding secrets for signing and encrypting p2panda operations.
///
/// Two key-pairs are managed by `Credentials`:
///
/// 1. Signing key: Give provenance and integrity guarantees for p2panda Operations.
/// 2. Identity key: Safe key agreement to learn about encryption secrets in p2panda Spaces.
///
/// **Warning:** Loosing any of these keys will lock you out of an encrypted space or any system
/// which requires authorization. _Never_ rotate one key and not the other when using spaces. If
/// you need to rotate your identity, rotate _both_ keys.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Credentials(Arc<Inner>);

#[derive(Debug, Serialize, Deserialize)]
struct Inner {
    signing_key: SigningKey,
    identity_secret_key: SecretKey,
}

// TODO: This currently uses test_utils in p2panda-encryption to allow low-level access like this.
// Ideally we should a) use rand::Rng trait instead of a concrete type in p2panda-encryption and b)
// make p2panda-core use the latest version of rand.
impl Credentials {
    /// Generate fresh signing and identity keys with your operating system's random number
    /// generator.
    pub fn generate() -> Self {
        Self::from_rng(&mut UnwrapErr(SysRng))
    }

    /// Take an existing signing key and generate a new random identity key.
    ///
    /// **Warning:** Use this method _only_ if you never persist data (during testing) or if you
    /// never use p2panda Spaces or if you need to upgrade from a previous version only using
    /// regular topic streams (unencrypted) into spaces streams (encrypted).
    pub fn from_signing_key(signing_key: SigningKey) -> Self {
        let mut rng = UnwrapErr(SysRng);

        let identity_secret_key = SecretKey::from_bytes({
            let mut bytes = [0; SECRET_KEY_SIZE];
            rng.fill_bytes(&mut bytes);
            bytes
        });

        Self::from_parts(signing_key, identity_secret_key)
    }

    /// Assemble `Credentials` from existing secrets.
    //
    // TODO: SecretKey doesn't have a from_bytes method like SigningKey. We need to see if we want
    // to remove it or make it public.
    pub fn from_parts(signing_key: SigningKey, identity_secret_key: SecretKey) -> Self {
        let inner = Inner {
            signing_key,
            identity_secret_key,
        };

        Self(Arc::new(inner))
    }

    /// Generate fresh signing and identity keys given a custom random number generator.
    pub fn from_rng<R: Rng>(rng: &mut R) -> Self {
        let signing_key = SigningKey::from({
            let mut bytes = [0; SIGNING_KEY_LEN];
            rng.fill_bytes(&mut bytes);
            bytes
        });

        let identity_secret_key = SecretKey::from_bytes({
            let mut bytes = [0; SECRET_KEY_SIZE];
            rng.fill_bytes(&mut bytes);
            bytes
        });

        Self::from_parts(signing_key, identity_secret_key)
    }

    /// Public verifying key used for verifying signatures, derived from secret signing key.
    pub fn verifying_key(&self) -> VerifyingKey {
        self.0.signing_key.verifying_key()
    }

    /// Public identity key used for key agreement, derived from secret identity key.
    pub fn identity_key(&self) -> Result<PublicKey, X25519Error> {
        self.0.identity_secret_key.verifying_key()
    }

    /// Secret signing key used in networking protocols to authenticate ourselves towards other
    /// nodes.
    pub(crate) fn node_signing_key(&self) -> SigningKey {
        // Currently we're using the same key to authenticate ourselves via TLS as we do for signing
        // append-only log operations (provenance).
        self.0.signing_key.clone()
    }

    /// Public key used as a node identity in networking protocols, derived from secret signing key.
    #[allow(unused)]
    pub(crate) fn node_id(&self) -> VerifyingKey {
        self.0.signing_key.verifying_key()
    }
}

impl Default for Credentials {
    fn default() -> Self {
        Self::generate()
    }
}

// Sign operations.
impl Signer for Credentials {
    fn sign(&self, bytes: &[u8]) -> p2panda_core::Signature {
        self.0.signing_key.sign(bytes)
    }

    fn verifying_key(&self) -> VerifyingKey {
        self.0.signing_key.verifying_key()
    }
}

// Sign & create key bundles for initial key agreement in p2panda Spaces.
impl From<&Credentials> for p2panda_spaces::Credentials {
    fn from(credentials: &Credentials) -> Self {
        Self::from_keys(
            credentials.0.signing_key.clone(),
            credentials.0.identity_secret_key.clone(),
        )
    }
}
