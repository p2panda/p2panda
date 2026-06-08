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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Credentials(Arc<Inner>);

#[derive(Debug, Serialize, Deserialize)]
struct Inner {
    signing_key: SigningKey,
    identity_secret_key: SecretKey,
}

impl Credentials {
    pub fn generate() -> Self {
        Self::from_rng(&mut UnwrapErr(SysRng))
    }

    pub fn from_rng<R: Rng>(rng: &mut R) -> Self {
        // TODO: This currently uses test_utils in p2panda-encryption to allow low-level access like
        // this. Ideally we should a) use rand::Rng trait instead of a concrete type in
        // p2panda-encryption and b) make p2panda-core use the latest version of rand.
        let inner = Inner {
            signing_key: SigningKey::from({
                let mut bytes = [0; SIGNING_KEY_LEN];
                rng.fill_bytes(&mut bytes);
                bytes
            }),
            identity_secret_key: SecretKey::from_bytes({
                let mut bytes = [0; SECRET_KEY_SIZE];
                rng.fill_bytes(&mut bytes);
                bytes
            }),
        };

        Self(Arc::new(inner))
    }

    pub fn verifying_key(&self) -> VerifyingKey {
        self.0.signing_key.verifying_key()
    }

    pub fn identity_key(&self) -> Result<PublicKey, X25519Error> {
        self.0.identity_secret_key.verifying_key()
    }

    pub(crate) fn node_signing_key(&self) -> SigningKey {
        // Currently we're using the same key to authenticate ourselves via TLS as we do for signing
        // append-only log operations (provenance).
        self.0.signing_key.clone()
    }

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

impl Signer for Credentials {
    fn sign(&self, bytes: &[u8]) -> p2panda_core::Signature {
        self.0.signing_key.sign(bytes)
    }
}
