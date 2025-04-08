// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashMap;
use std::fmt;

use p2panda_core::cbor::{DecodeError, EncodeError, decode_cbor, encode_cbor};
use serde::de::{SeqAccess, Visitor};
use serde::ser::SerializeSeq;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::crypto::Secret;
use crate::crypto::sha2::sha2_256;
use crate::{Rng, RngError};

/// 256-bit secret group key.
pub const GROUP_SECRET_SIZE: usize = 32;

pub type GroupSecretId = [u8; 32];

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(any(test, feature = "test_utils"), derive(Clone))]
pub struct GroupSecret(Secret<GROUP_SECRET_SIZE>);

impl GroupSecret {
    pub(crate) fn from_rng(rng: &Rng) -> Result<Self, GroupSecretError> {
        let bytes: [u8; GROUP_SECRET_SIZE] = rng.random_array()?;
        Ok(Self(Secret::from_bytes(bytes)))
    }

    pub(crate) fn from_bytes(bytes: [u8; GROUP_SECRET_SIZE]) -> Self {
        Self(Secret::from_bytes(bytes))
    }

    pub fn try_from_bytes(bytes: &[u8]) -> Result<Self, GroupSecretError> {
        let bytes: [u8; GROUP_SECRET_SIZE] = bytes
            .try_into()
            .map_err(|_| GroupSecretError::InvalidKeySize)?;
        Ok(Self::from_bytes(bytes))
    }

    pub fn id(&self) -> GroupSecretId {
        sha2_256(&[self.0.as_bytes()])
    }

    pub(crate) fn as_bytes(&self) -> &[u8; GROUP_SECRET_SIZE] {
        self.0.as_bytes()
    }
}

#[derive(Debug, PartialEq, Eq)]
#[cfg_attr(any(test, feature = "test_utils"), derive(Clone))]
pub struct GroupSecretBundle(HashMap<GroupSecretId, GroupSecret>);

impl GroupSecretBundle {
    pub fn try_from_bytes(bytes: &[u8]) -> Result<Self, GroupSecretError> {
        let bundle: Self = decode_cbor(&bytes[..])?;
        Ok(bundle)
    }

    pub(crate) fn from_secrets(secrets: Vec<GroupSecret>) -> Self {
        let mut bundle = HashMap::new();
        for secret in secrets {
            bundle.insert(secret.id(), secret);
        }
        Self(bundle)
    }

    pub(crate) fn to_bytes(&self) -> Result<Vec<u8>, GroupSecretError> {
        let bytes = encode_cbor(self)?;
        Ok(bytes)
    }
}

impl Serialize for GroupSecretBundle {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut s = serializer.serialize_seq(Some(self.0.len()))?;
        for secret in self.0.values() {
            s.serialize_element(secret)?;
        }
        s.end()
    }
}

impl<'de> Deserialize<'de> for GroupSecretBundle {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct SecretVisitor;

        impl<'de> Visitor<'de> for SecretVisitor {
            type Value = Vec<GroupSecret>;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("Header encoded as a sequence")
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                let mut result = Vec::new();
                while let Some(secret) = seq.next_element()? {
                    result.push(secret);
                }
                Ok(result)
            }
        }

        let secrets = deserializer.deserialize_seq(SecretVisitor)?;

        Ok(GroupSecretBundle::from_secrets(secrets))
    }
}

#[derive(Debug, Error)]
pub enum GroupSecretError {
    #[error("the given key does not match the required 32 byte length")]
    InvalidKeySize,

    #[error(transparent)]
    Rng(#[from] RngError),

    #[error(transparent)]
    Encode(#[from] EncodeError),

    #[error(transparent)]
    Decode(#[from] DecodeError),
}
