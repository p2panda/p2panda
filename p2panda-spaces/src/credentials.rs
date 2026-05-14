// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_core::{SigningKey, VerifyingKey};
use p2panda_encryption::crypto::x25519::SecretKey;
use p2panda_encryption::{Rng, RngError};
use serde::{Deserialize, Serialize};

/// A member's private key and identity secret.
///
/// Every peer has two secret keys; _both_ are required in order to interact with p2panda-spaces
/// and neither can be rotated without losing access to all spaces. _If_ key rotation is required
/// then both keys should be rotated together.
///
/// A peers' identity secret is used for key agreement in p2panda-encryption. Their private key is
/// used to sign messages and the associated public key is used to identify the peer (eg. for
/// access control purposes).
#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "test_utils"), derive(Clone))]
pub struct Credentials {
    pub(crate) signing_key: SigningKey,
    pub(crate) identity_secret: SecretKey,
}

impl Credentials {
    pub fn from_rng(rng: &Rng) -> Result<Self, RngError> {
        let signing_key = SigningKey::from_bytes(&rng.random_array()?);
        let identity_secret = SecretKey::from_rng(rng)?;
        Ok(Self {
            signing_key,
            identity_secret,
        })
    }

    pub fn from_keys(signing_key: SigningKey, identity_secret: SecretKey) -> Self {
        Self {
            signing_key,
            identity_secret,
        }
    }

    pub fn signing_key(&self) -> SigningKey {
        self.signing_key.clone()
    }

    pub fn verifying_key(&self) -> VerifyingKey {
        self.signing_key.verifying_key()
    }

    pub fn identity_secret(&self) -> SecretKey {
        self.identity_secret.clone()
    }
}
