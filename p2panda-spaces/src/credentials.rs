// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_core::{PrivateKey, PublicKey};
use p2panda_encryption::crypto::x25519::SecretKey;
use p2panda_encryption::{Rng, RngError};
use serde::{Deserialize, Serialize};

/// Every peer has two secret keys and _both_ required in order to interact with p2panda-spaces
/// and neither can be rotated without losing access to all spaces. _If_ key rotation is required
/// then both keys should be rotated together.
///  
/// A peers' identity secret is used for key agreement in p2panda-encryption. Their private key is
/// used to sign messages and the associated public key is used to identify the peer (eg. for
/// access control purposes).  
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Credentials {
    pub(crate) private_key: PrivateKey,
    pub(crate) identity_secret: SecretKey,
}

impl Credentials {
    pub fn new(rng: &Rng) -> Result<Self, RngError> {
        let private_key = PrivateKey::from_bytes(&rng.random_array()?);
        let identity_secret = SecretKey::from_rng(rng)?;
        Ok(Self {
            private_key,
            identity_secret,
        })
    }

    pub fn from_keys(private_key: PrivateKey, identity_secret: SecretKey) -> Self {
        Self {
            private_key,
            identity_secret,
        }
    }

    pub fn private_key(&self) -> PrivateKey {
        self.private_key.clone()
    }

    pub fn public_key(&self) -> PublicKey {
        self.private_key.public_key()
    }

    pub fn identity_secret(&self) -> SecretKey {
        self.identity_secret.clone()
    }
}
