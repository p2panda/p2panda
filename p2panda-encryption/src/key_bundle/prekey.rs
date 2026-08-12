// SPDX-License-Identifier: MIT OR Apache-2.0

use serde::{Deserialize, Serialize};

use crate::crypto::Rng;
use crate::crypto::x25519::{PUBLIC_KEY_SIZE, PublicKey, SecretKey};
use crate::crypto::xeddsa::{XEdDSAError, XSignature, xeddsa_sign};
use crate::key_bundle::{Lifetime, LifetimeError};

/// Pre-key with key material for X3DH key agreement to be used until it's lifetime has expired.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreKey(PublicKey, Lifetime);

/// Unique identifier of a member's pre-key which can be used to address long-term key bundles.
pub type PreKeyId = PublicKey;

impl PreKey {
    pub fn new(prekey: PublicKey, lifetime: Lifetime) -> Self {
        Self(prekey, lifetime)
    }

    pub fn key(&self) -> &PublicKey {
        &self.0
    }

    pub fn as_bytes(&self) -> &[u8; PUBLIC_KEY_SIZE] {
        self.0.as_bytes()
    }

    pub fn to_bytes(self) -> [u8; PUBLIC_KEY_SIZE] {
        self.0.to_bytes()
    }

    pub fn sign(&self, secret_key: &SecretKey, rng: &Rng) -> Result<XSignature, XEdDSAError> {
        xeddsa_sign(self.0.as_bytes(), secret_key, rng)
    }

    pub fn lifetime(&self) -> &Lifetime {
        &self.1
    }

    pub fn verify_lifetime(&self) -> Result<(), LifetimeError> {
        self.1.verify()
    }
}

/// Unique identifier of a member's one-time pre-key.
pub type OneTimePreKeyId = u64;

/// Pre-key with key material for X3DH key agreement to be used exactly _once_.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OneTimePreKey(PublicKey, OneTimePreKeyId);

impl OneTimePreKey {
    pub fn new(onetime_prekey: PublicKey, id: OneTimePreKeyId) -> Self {
        Self(onetime_prekey, id)
    }

    pub fn key(&self) -> &PublicKey {
        &self.0
    }

    pub fn id(&self) -> OneTimePreKeyId {
        self.1
    }

    pub fn as_bytes(&self) -> &[u8; PUBLIC_KEY_SIZE] {
        self.0.as_bytes()
    }

    pub fn to_bytes(&self) -> [u8; PUBLIC_KEY_SIZE] {
        self.0.to_bytes()
    }
}

/// Helper method to identify the "latest" (valid and with furthest expiry date) pre-key from a
/// set. Returns `None` if no valid key was given.
pub fn latest_prekey<'a>(prekeys: Vec<&'a PreKey>) -> Option<&'a PreKey> {
    let mut latest: Option<&'a PreKey> = None;

    for prekey in prekeys {
        // Remove all prekeys which are _too early_ or _too late_ (expired).
        //
        //                   Now
        // too late --> [---] |
        //                    | [----] <-- too early
        //              [-----|----] <-- valid
        //                    |
        //
        //                  t -->
        //
        if prekey.lifetime().verify().is_err() {
            continue;
        }

        // Of all other, valid ones, find the one which has the "furthest" expiry date and is
        // therefore the "latest" key bundle.
        //
        //                   Now
        //                    |
        //                  [-|---------]
        //              [-----|------------] <-- "latest"
        //          [---------|-----]
        //                    |
        //
        //                  t -->
        //
        match latest {
            Some(current_prekey) => {
                if prekey.lifetime() > current_prekey.lifetime() {
                    latest = Some(prekey);
                }
            }
            None => {
                latest = Some(prekey);
            }
        }
    }

    latest
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use crate::Rng;
    use crate::crypto::x25519::SecretKey;
    use crate::key_bundle::Lifetime;

    use super::PreKey;

    #[test]
    fn latest_prekey() {
        let rng = Rng::from_seed([1; 32]);
        let public_key = SecretKey::from_rng(&rng).unwrap().verifying_key().unwrap();

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("SystemTime before UNIX EPOCH!")
            .as_secs();

        //      NOW
        // ===|=== (1)
        //   =|====== (2) <-- "latest"
        //    |      === (3)
        let prekey_1 = PreKey::new(public_key, Lifetime::from_range(now - 30, now + 30));
        let prekey_2 = PreKey::new(public_key, Lifetime::from_range(now - 10, now + 60));
        let prekey_3 = PreKey::new(public_key, Lifetime::from_range(now + 60, now + 90));

        let result = super::latest_prekey(vec![&prekey_1, &prekey_2, &prekey_3]);
        assert_eq!(result, Some(&prekey_2));
    }
}
