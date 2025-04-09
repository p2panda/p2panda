// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashMap;
use std::collections::hash_map::{IntoIter, Iter, Keys, Values};
use std::fmt;
use std::hash::Hash as StdHash;

use p2panda_core::cbor::{DecodeError, EncodeError, decode_cbor, encode_cbor};
use serde::de::{SeqAccess, Visitor};
use serde::ser::SerializeSeq;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::crypto::Secret;
use crate::crypto::sha2::{SHA256_DIGEST_SIZE, sha2_256};
use crate::{Rng, RngError};

/// 256-bit secret group key.
pub const GROUP_SECRET_SIZE: usize = 32;

pub type GroupSecretId = [u8; SHA256_DIGEST_SIZE];

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupSecret(Secret<GROUP_SECRET_SIZE>);

impl GroupSecret {
    pub(crate) fn from_rng(rng: &Rng) -> Result<Self, GroupSecretError> {
        let bytes: [u8; GROUP_SECRET_SIZE] = rng.random_array()?;
        Ok(Self(Secret::from_bytes(bytes)))
    }

    pub(crate) fn from_bytes(bytes: [u8; GROUP_SECRET_SIZE]) -> Self {
        Self(Secret::from_bytes(bytes))
    }

    pub(crate) fn try_from_bytes(bytes: &[u8]) -> Result<Self, GroupSecretError> {
        let bytes: [u8; GROUP_SECRET_SIZE] = bytes
            .try_into()
            .map_err(|_| GroupSecretError::InvalidKeySize)?;
        Ok(Self::from_bytes(bytes))
    }

    /// Returns identifier (SHA256 hash) for this secret.
    pub fn id(&self) -> GroupSecretId {
        sha2_256(&[self.0.as_bytes()])
    }

    pub(crate) fn as_bytes(&self) -> &[u8; GROUP_SECRET_SIZE] {
        self.0.as_bytes()
    }
}

impl StdHash for GroupSecret {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id().hash(state);
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GroupSecretBundle(HashMap<GroupSecretId, GroupSecret>);

impl GroupSecretBundle {
    pub fn new() -> Self {
        Self(HashMap::new())
    }

    pub fn from_secrets(secrets: Vec<GroupSecret>) -> Self {
        Self(HashMap::from_iter(
            secrets.into_iter().map(|secret| (secret.id(), secret)),
        ))
    }

    pub(crate) fn try_from_bytes(bytes: &[u8]) -> Result<Self, GroupSecretError> {
        Ok(decode_cbor(bytes)?)
    }

    pub(crate) fn to_bytes(&self) -> Result<Vec<u8>, GroupSecretError> {
        Ok(encode_cbor(self)?)
    }

    pub fn insert(&mut self, secret: GroupSecret) {
        self.0.insert(secret.id(), secret);
    }

    pub fn get(&self, id: &GroupSecretId) -> Option<&GroupSecret> {
        self.0.get(id)
    }

    pub fn remove(&mut self, id: &GroupSecretId) -> Option<GroupSecret> {
        self.0.remove(id)
    }

    pub fn extend(&mut self, bundle: GroupSecretBundle) {
        self.0.extend(bundle.0);
    }

    pub fn contains(&mut self, id: &GroupSecretId) -> bool {
        self.0.contains_key(id)
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn iter(&self) -> Iter<'_, GroupSecretId, GroupSecret> {
        self.0.iter()
    }

    pub fn into_iter(self) -> IntoIter<GroupSecretId, GroupSecret> {
        self.0.into_iter()
    }

    pub fn ids(&self) -> Keys<'_, GroupSecretId, GroupSecret> {
        self.0.keys()
    }

    pub fn secrets(&self) -> Values<'_, GroupSecretId, GroupSecret> {
        self.0.values()
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
        struct SecretListVisitor;

        impl<'de> Visitor<'de> for SecretListVisitor {
            type Value = Vec<GroupSecret>;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("list of group secrets")
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

        let secrets = deserializer.deserialize_seq(SecretListVisitor)?;

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

#[cfg(test)]
mod tests {
    use crate::Rng;

    use super::{GroupSecret, GroupSecretBundle};

    #[test]
    fn group_secret_bundle() {
        let rng = Rng::from_seed([1; 32]);

        let secret = GroupSecret::from_rng(&rng).unwrap();
        assert_ne!(secret.as_bytes(), &secret.id());

        let mut bundle_1 = GroupSecretBundle::from_secrets(vec![secret.clone()]);
        let mut bundle_2 = GroupSecretBundle::from_secrets(vec![secret.clone()]);
        assert_eq!(bundle_1.len(), 1);
        assert_eq!(bundle_2.len(), 1);

        assert_eq!(bundle_1.get(&secret.id()), bundle_2.get(&secret.id()));
        assert!(bundle_1.get(&secret.id()).is_some());
        assert!(bundle_1.contains(&secret.id()));

        let unknown_secret = GroupSecret::from_rng(&rng).unwrap();
        assert!(bundle_1.get(&unknown_secret.id()).is_none());
        assert!(!bundle_1.contains(&unknown_secret.id()));

        let secret_2 = GroupSecret::from_rng(&rng).unwrap();
        bundle_2.insert(secret_2.clone());
        assert_eq!(bundle_2.len(), 2);

        bundle_1.extend(bundle_2);
        assert_eq!(bundle_1.len(), 2);

        assert!(bundle_1.remove(&secret_2.id()).is_some());
        assert_eq!(bundle_1.len(), 1);
    }

    #[test]
    fn serde() {
        let rng = Rng::from_seed([1; 32]);

        let bundle = GroupSecretBundle::from_secrets(vec![
            GroupSecret::from_rng(&rng).unwrap(),
            GroupSecret::from_rng(&rng).unwrap(),
            GroupSecret::from_rng(&rng).unwrap(),
            GroupSecret::from_rng(&rng).unwrap(),
        ]);

        let bytes = bundle.to_bytes().unwrap();
        assert_eq!(bundle, GroupSecretBundle::try_from_bytes(&bytes).unwrap());
    }
}
