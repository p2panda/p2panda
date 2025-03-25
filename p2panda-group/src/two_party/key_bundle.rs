// SPDX-License-Identifier: MIT OR Apache-2.0

use serde::{Deserialize, Serialize};

use crate::crypto::Rng;
use crate::crypto::x25519::{PUBLIC_KEY_SIZE, PublicKey, SecretKey};
use crate::crypto::xeddsa::{XEdDSAError, XSignature, xeddsa_sign, xeddsa_verify};
use crate::traits::{KeyBundle, OneTimeKeyId};
use crate::two_party::X3DHError;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreKey(PublicKey);

impl PreKey {
    pub fn new(prekey: PublicKey) -> Self {
        Self(prekey)
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OneTimeKeyBundle {
    identity_key: PublicKey,
    signed_prekey: PreKey,
    prekey_signature: XSignature,
    onetime_prekey: Option<OneTimeKey>,
}

impl OneTimeKeyBundle {
    pub fn new(
        identity_key: PublicKey,
        signed_prekey: PreKey,
        prekey_signature: XSignature,
        onetime_prekey: Option<OneTimeKey>,
    ) -> Self {
        Self {
            identity_key,
            signed_prekey,
            prekey_signature,
            onetime_prekey,
        }
    }
}

impl KeyBundle for OneTimeKeyBundle {
    type Error = X3DHError;

    fn identity_key(&self) -> &PublicKey {
        &self.identity_key
    }

    fn signed_prekey(&self) -> &PublicKey {
        &self.signed_prekey.0
    }

    fn onetime_prekey(&self) -> Option<&PublicKey> {
        self.onetime_prekey.as_ref().map(|key| &key.0)
    }

    fn onetime_prekey_id(&self) -> Option<OneTimeKeyId> {
        self.onetime_prekey.as_ref().map(|key| key.1)
    }

    fn verify(&self) -> Result<(), Self::Error> {
        xeddsa_verify(
            self.signed_prekey.as_bytes(),
            &self.identity_key,
            &self.prekey_signature,
        )?;
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LongTermKeyBundle {
    identity_key: PublicKey,
    signed_prekey: PreKey,
    prekey_signature: XSignature,
}

impl LongTermKeyBundle {
    pub fn new(
        identity_key: PublicKey,
        signed_prekey: PreKey,
        prekey_signature: XSignature,
    ) -> Self {
        Self {
            identity_key,
            signed_prekey,
            prekey_signature,
        }
    }
}

impl KeyBundle for LongTermKeyBundle {
    type Error = X3DHError;

    fn identity_key(&self) -> &PublicKey {
        &self.identity_key
    }

    fn signed_prekey(&self) -> &PublicKey {
        &self.signed_prekey.0
    }

    fn onetime_prekey(&self) -> Option<&PublicKey> {
        None
    }

    fn onetime_prekey_id(&self) -> Option<OneTimeKeyId> {
        None
    }

    fn verify(&self) -> Result<(), X3DHError> {
        xeddsa_verify(
            self.signed_prekey.as_bytes(),
            &self.identity_key,
            &self.prekey_signature,
        )?;
        Ok(())
    }
}
