// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashMap;
use std::collections::hash_map::{IntoIter, Iter, Keys, Values};
use std::fmt;
use std::hash::Hash as StdHash;
use std::time::{SystemTime, SystemTimeError, UNIX_EPOCH};

use p2panda_core::cbor::{DecodeError, EncodeError, decode_cbor, encode_cbor};
use serde::de::{SeqAccess, Visitor};
use serde::ser::SerializeSeq;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::crypto::sha2::{SHA256_DIGEST_SIZE, sha2_256};
use crate::crypto::{Rng, RngError, Secret};

/// 256-bit secret group key.
pub const GROUP_SECRET_SIZE: usize = 32;

pub type GroupSecretId = [u8; SHA256_DIGEST_SIZE];

pub type Timestamp = u64;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupSecret(Secret<GROUP_SECRET_SIZE>, Timestamp);

impl GroupSecret {
    #[cfg(any(test, feature = "test_utils"))]
    pub(crate) fn new(bytes: [u8; GROUP_SECRET_SIZE], timestamp: Timestamp) -> Self {
        Self(Secret::from_bytes(bytes), timestamp)
    }

    /// Create a new group secret with current timestamp from random-number generator.
    pub(crate) fn from_rng(rng: &Rng) -> Result<Self, GroupSecretError> {
        let bytes: [u8; GROUP_SECRET_SIZE] = rng.random_array()?;
        Self::from_bytes(bytes)
    }

    /// Create a new group secret with current timestamp from random byte string.
    pub(crate) fn from_bytes(bytes: [u8; GROUP_SECRET_SIZE]) -> Result<Self, GroupSecretError> {
        let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
        Ok(Self(Secret::from_bytes(bytes), now))
    }

    /// Deserialize group secret from CBOR representation.
    pub(crate) fn try_from_bytes(bytes: &[u8]) -> Result<Self, GroupSecretError> {
        Ok(decode_cbor(bytes)?)
    }

    /// Returns identifier (SHA256 hash) for this secret.
    pub fn id(&self) -> GroupSecretId {
        sha2_256(&[self.0.as_bytes()])
    }

    /// Return creation date (UNIX timestamp in seconds) of this secret.
    pub fn timestamp(&self) -> Timestamp {
        self.1
    }

    /// Returns secret key as bytes.
    pub fn as_bytes(&self) -> &[u8; GROUP_SECRET_SIZE] {
        self.0.as_bytes()
    }

    /// Serialize group secret into CBOR representation.
    pub(crate) fn to_bytes(&self) -> Result<Vec<u8>, GroupSecretError> {
        Ok(encode_cbor(self)?)
    }
}

impl StdHash for GroupSecret {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id().hash(state);
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GroupSecretBundle {
    secrets: HashMap<GroupSecretId, GroupSecret>,
    latest: Option<GroupSecretId>,
}

impl GroupSecretBundle {
    pub fn new() -> Self {
        Self {
            secrets: HashMap::new(),
            latest: None,
        }
    }

    pub fn from_secrets(secrets: Vec<GroupSecret>) -> Self {
        let secrets = HashMap::from_iter(secrets.into_iter().map(|secret| (secret.id(), secret)));
        Self {
            latest: find_latest(&secrets),
            secrets,
        }
    }

    pub(crate) fn try_from_bytes(bytes: &[u8]) -> Result<Self, GroupSecretError> {
        Ok(decode_cbor(bytes)?)
    }

    pub(crate) fn to_bytes(&self) -> Result<Vec<u8>, GroupSecretError> {
        Ok(encode_cbor(self)?)
    }

    pub fn latest(&self) -> Option<&GroupSecret> {
        self.latest.as_ref().and_then(|id| self.secrets.get(id))
    }

    pub fn insert(&mut self, secret: GroupSecret) {
        self.secrets.insert(secret.id(), secret);
        self.latest = find_latest(&self.secrets);
    }

    pub fn get(&self, id: &GroupSecretId) -> Option<&GroupSecret> {
        self.secrets.get(id)
    }

    pub fn remove(&mut self, id: &GroupSecretId) -> Option<GroupSecret> {
        let result = self.secrets.remove(id);
        self.latest = find_latest(&self.secrets);
        result
    }

    pub fn extend(&mut self, bundle: GroupSecretBundle) {
        self.secrets.extend(bundle.secrets);
        self.latest = find_latest(&self.secrets);
    }

    pub fn contains(&mut self, id: &GroupSecretId) -> bool {
        self.secrets.contains_key(id)
    }

    pub fn len(&self) -> usize {
        self.secrets.len()
    }

    pub fn is_empty(&self) -> bool {
        self.secrets.is_empty()
    }

    pub fn iter(&self) -> Iter<'_, GroupSecretId, GroupSecret> {
        self.secrets.iter()
    }

    pub fn into_iter(self) -> IntoIter<GroupSecretId, GroupSecret> {
        self.secrets.into_iter()
    }

    pub fn ids(&self) -> Keys<'_, GroupSecretId, GroupSecret> {
        self.secrets.keys()
    }

    pub fn secrets(&self) -> Values<'_, GroupSecretId, GroupSecret> {
        self.secrets.values()
    }
}

/// Finds the "latest" secret to use from a list by comparing timestamps. If the timestamps of two
/// distinct secrets match the id is used as a tie-breaker.
fn find_latest(secrets: &HashMap<GroupSecretId, GroupSecret>) -> Option<GroupSecretId> {
    let mut latest_timestamp: Timestamp = 0;
    let mut latest_secret_id: Option<GroupSecretId> = None;
    for (id, secret) in secrets {
        let timestamp = secret.timestamp();
        if latest_timestamp < timestamp
            || (latest_timestamp == timestamp
                && *id > latest_secret_id.unwrap_or([0; SHA256_DIGEST_SIZE]))
        {
            latest_timestamp = timestamp;
            latest_secret_id = Some(id.to_owned());
        }
    }
    latest_secret_id
}

impl Serialize for GroupSecretBundle {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut s = serializer.serialize_seq(Some(self.secrets.len()))?;
        for secret in self.secrets.values() {
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

    #[error(transparent)]
    SystemTime(#[from] SystemTimeError),
}

#[cfg(test)]
mod tests {
    use crate::Rng;

    use super::{GroupSecret, GroupSecretBundle};

    #[test]
    fn group_secret_bundle() {
        let rng = Rng::from_seed([1; 32]);

        let secret = GroupSecret::from_rng(&rng).unwrap();

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
    fn latest_secret() {
        let mut bundle = GroupSecretBundle::new();
        assert!(bundle.latest().is_none());

        let secret_1 = GroupSecret::new([1; 32], 234);
        assert_eq!(secret_1.timestamp(), 234);
        let secret_2 = GroupSecret::new([2; 32], 234); // same timestamp
        assert_eq!(secret_2.timestamp(), 234);
        let secret_3 = GroupSecret::new([3; 32], 345);
        assert_eq!(secret_3.timestamp(), 345);
        let secret_4 = GroupSecret::new([4; 32], 123);
        assert_eq!(secret_4.timestamp(), 123);

        // Inserted secret 1 is the latest.
        bundle.insert(secret_1.clone());
        assert_eq!(bundle.len(), 1);
        assert_eq!(bundle.latest(), Some(&secret_1));

        // Inserted secret 2 is the "latest" as the higher hash wins when both timestamps are the
        // same.
        bundle.insert(secret_2.clone());
        assert_eq!(bundle.len(), 2);
        assert_eq!(bundle.latest(), Some(&secret_2));

        // Use a separate group to confirm that the order of insertion does not matter here.
        {
            let mut bundle_2 = GroupSecretBundle::new();
            bundle_2.insert(secret_2.clone());
            bundle_2.insert(secret_1.clone());
            assert_eq!(bundle_2.latest(), Some(&secret_2));
        }

        // Inserted 3 is the latest.
        bundle.insert(secret_3.clone());
        assert_eq!(bundle.len(), 3);
        assert_eq!(bundle.latest(), Some(&secret_3));

        // Inserted 3 is still the latest.
        bundle.insert(secret_4.clone());
        assert_eq!(bundle.len(), 4);
        assert_eq!(bundle.latest(), Some(&secret_3));
    }

    #[test]
    fn serde() {
        let rng = Rng::from_seed([1; 32]);

        // Serialize & deserialize bundle.
        let bundle = GroupSecretBundle::from_secrets(vec![
            GroupSecret::from_rng(&rng).unwrap(),
            GroupSecret::from_rng(&rng).unwrap(),
            GroupSecret::from_rng(&rng).unwrap(),
            GroupSecret::from_rng(&rng).unwrap(),
        ]);

        let bytes = bundle.to_bytes().unwrap();
        assert_eq!(bundle, GroupSecretBundle::try_from_bytes(&bytes).unwrap());

        // Serialize & deserialize single secret.
        let secret = GroupSecret::from_rng(&rng).unwrap();
        let timestamp = secret.timestamp();

        let bytes = secret.to_bytes().unwrap();
        let secret_again = GroupSecret::try_from_bytes(&bytes).unwrap();
        assert_eq!(secret, secret_again);
        assert_eq!(timestamp, secret_again.timestamp());
    }
}
