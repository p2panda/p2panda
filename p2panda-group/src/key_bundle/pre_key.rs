// SPDX-License-Identifier: MIT OR Apache-2.0

use serde::{Deserialize, Serialize};

use crate::crypto::Rng;
use crate::crypto::x25519::{PUBLIC_KEY_SIZE, PublicKey, SecretKey};
use crate::crypto::xeddsa::{XEdDSAError, XSignature, xeddsa_sign};
use crate::key_bundle::{Lifetime, LifetimeError};

pub type OneTimeKeyId = u64;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreKey(PublicKey, Lifetime);

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

    pub fn verify_lifetime(&self) -> Result<(), LifetimeError> {
        self.1.verify()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OneTimeKey(PublicKey, OneTimeKeyId);

impl OneTimeKey {
    pub fn new(onetime_prekey: PublicKey, id: OneTimeKeyId) -> Self {
        Self(onetime_prekey, id)
    }

    pub fn key(&self) -> &PublicKey {
        &self.0
    }

    pub fn id(&self) -> OneTimeKeyId {
        self.1
    }

    pub fn as_bytes(&self) -> &[u8; PUBLIC_KEY_SIZE] {
        self.0.as_bytes()
    }

    pub fn to_bytes(&self) -> [u8; PUBLIC_KEY_SIZE] {
        self.0.to_bytes()
    }
}
