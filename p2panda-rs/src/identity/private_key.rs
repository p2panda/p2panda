// SPDX-License-Identifier: AGPL-3.0-or-later

use std::convert::TryInto;
use std::fmt::Display;

use ed25519_dalek::{SigningKey, SECRET_KEY_LENGTH};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};

use crate::serde::{deserialize_hex, serialize_hex};

#[derive(Clone, Debug)]
pub struct PrivateKey(SigningKey);

impl PrivateKey {
    pub fn new() -> Self {
        let mut csprng: OsRng = OsRng;
        let private_key = SigningKey::generate(&mut csprng);
        Self(private_key)
    }

    /// Returns private key represented as bytes.
    pub fn to_bytes(&self) -> [u8; SECRET_KEY_LENGTH] {
        self.0.to_bytes()
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
