// SPDX-License-Identifier: AGPL-3.0-or-later

use std::convert::TryInto;
use std::fmt::Display;

use ed25519_dalek_v2::{Signer, SigningKey, SECRET_KEY_LENGTH};
use rand_v2::rngs::OsRng;
use serde::{Deserialize, Serialize};

use crate::identity_v2::error::PrivateKeyError;
use crate::identity_v2::{PublicKey, Signature};
use crate::serde::{deserialize_hex, serialize_hex};

/// Private ed25519 key used for signing.
#[derive(Clone, Debug)]
pub struct PrivateKey(SigningKey);

impl PrivateKey {
    /// Generates a new private key using the systems random number generator (CSPRNG) as a seed.
    ///
    /// This uses `getrandom` for random number generation internally. See [`getrandom`] crate for
    /// supported platforms.
    ///
    /// **WARNING:** Depending on the context this does not guarantee the random number generator
    /// to be cryptographically secure (eg. broken / hijacked browser or system implementations),
    /// so make sure to only run this in trusted environments.
    ///
    /// [`getrandom`]: https://docs.rs/getrandom/0.2.1/getrandom/
    pub fn new() -> Self {
        let mut csprng: OsRng = OsRng;
        let private_key = SigningKey::generate(&mut csprng);
        Self(private_key)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, PrivateKeyError> {
        let private_key_bytes: [u8; SECRET_KEY_LENGTH] = bytes
            .try_into()
            .map_err(|_| PrivateKeyError::InvalidLength)?;

        Ok(Self(SigningKey::from_bytes(&private_key_bytes)))
    }

    /// Returns private key represented as bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        self.0.to_bytes().to_vec()
    }

    pub fn public_key(&self) -> PublicKey {
        self.0.verifying_key().into()
    }

    pub fn sign(&self, bytes: &[u8]) -> Signature {
        (&self.0.sign(bytes)).into()
    }
}

impl Display for PrivateKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", hex::encode(self.to_bytes()))
    }
}

impl Serialize for PrivateKey {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serialize_hex(&self.to_bytes(), serializer)
    }
}

impl<'de> Deserialize<'de> for PrivateKey {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let bytes = deserialize_hex(deserializer)?;
        let private_key_bytes: [u8; SECRET_KEY_LENGTH] = bytes
            .try_into()
            .map_err(|_| serde::de::Error::custom(format!("invalid private key bytes")))?;
        Ok(Self(SigningKey::from_bytes(&private_key_bytes)))
    }
}
