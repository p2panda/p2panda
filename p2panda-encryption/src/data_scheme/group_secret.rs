// SPDX-License-Identifier: MIT OR Apache-2.0

//! Methods to create and maintain multiple secrets known by a group which are used to encrypt and decrypt data.
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

/// Public identifier for each secret. This is the SHA256 digest of the secret key itself.
///
/// Can be used to help the receiver of a ciphertext to understand which key they can use to
/// decrypt the message.
pub type GroupSecretId = [u8; SHA256_DIGEST_SIZE];

/// UNIX timestamp indicating when the key was generated.
///
/// This helps peers to pick the "latest" key or remove keys based on their age (for forward secrecy)
/// depending on the application. If other ordering strategies are applied by the application they
/// can also be used instead to reason about the "latest" group secret.
pub type Timestamp = u64;

/// Secret known by a group which is used to encrypt and decrypt data.
///
/// Group secrets can be used multiple times and are dropped never or manually by the application,
/// thus providing a weaker forward secrecy than p2panda's "message encryption" scheme.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupSecret(Secret<GROUP_SECRET_SIZE>, Timestamp);

impl GroupSecret {
    #[cfg(any(test, feature = "test_utils"))]
    pub fn new(bytes: [u8; GROUP_SECRET_SIZE], timestamp: Timestamp) -> Self {
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

    /// Adjust the timestamp.
    pub(crate) fn set_timestamp(&mut self, timestamp: Timestamp) {
        self.1 = timestamp;
    }

    /// Returns secret key as bytes.
    pub(crate) fn as_bytes(&self) -> &[u8; GROUP_SECRET_SIZE] {
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

/// Bundle of all secrets used by a group to encrypt and decrypt it's data.
///
/// Peers manage all secrets they generated or learned about in this bundle. New secrets are added
/// to the bundle when a group got updated, a member was added or removed.
///
/// Secrets inside the bundle can be removed if the application considers them due, otherwise
/// a bundle will grow in size and no forward secrecy is given.
#[derive(Debug)]
pub struct SecretBundle;

#[derive(Debug, PartialEq, Eq)]
#[cfg_attr(any(test, feature = "test_utils"), derive(Clone))]
pub struct SecretBundleState {
    secrets: HashMap<GroupSecretId, GroupSecret>,
    latest: Option<GroupSecretId>,
}

impl SecretBundleState {
    /// Returns the latest known secret which should preferably used for encrypting new data.
    pub fn latest(&self) -> Option<&GroupSecret> {
        self.latest.as_ref().and_then(|id| self.secrets.get(id))
    }

    /// Returns a secret based on the id.
    ///
    /// This can be used to retrieve a secret to decrypt data where we know which secret id has been
    /// used.
    pub fn get(&self, id: &GroupSecretId) -> Option<&GroupSecret> {
        self.secrets.get(id)
    }

    /// Returns true when the bundle contains a secret with the given id.
    pub fn contains(&self, id: &GroupSecretId) -> bool {
        self.secrets.contains_key(id)
    }

    /// Returns number of all secrets in this bundle.
    pub fn len(&self) -> usize {
        self.secrets.len()
    }

    /// Returns true if there's no secrets in this bundle.
    pub fn is_empty(&self) -> bool {
        self.secrets.is_empty()
    }

    /// Iterator over secrets (values) and their ids (keys).
    pub fn iter(&self) -> Iter<'_, GroupSecretId, GroupSecret> {
        self.secrets.iter()
    }

    /// Iterator over all ids.
    pub fn ids(&self) -> Keys<'_, GroupSecretId, GroupSecret> {
        self.secrets.keys()
    }

    /// Iterator over all secrets.
    pub fn secrets(&self) -> Values<'_, GroupSecretId, GroupSecret> {
        self.secrets.values()
    }

    /// Encodes bundle in CBOR format.
    pub(crate) fn to_bytes(&self) -> Result<Vec<u8>, GroupSecretError> {
        Ok(encode_cbor(self)?)
    }
}

impl std::iter::IntoIterator for SecretBundleState {
    type Item = (GroupSecretId, GroupSecret);

    type IntoIter = IntoIter<GroupSecretId, GroupSecret>;

    fn into_iter(self) -> Self::IntoIter {
        self.secrets.into_iter()
    }
}

impl Serialize for SecretBundleState {
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

impl<'de> Deserialize<'de> for SecretBundleState {
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

        Ok(SecretBundle::from_secrets(secrets))
    }
}

impl SecretBundle {
    /// Initialises empty secret bundle state.
    pub fn init() -> SecretBundleState {
        SecretBundleState {
            secrets: HashMap::new(),
            latest: None,
        }
    }

    /// Initialises secret bundle state from a list of group secrets.
    pub fn from_secrets(secrets: Vec<GroupSecret>) -> SecretBundleState {
        let secrets = HashMap::from_iter(secrets.into_iter().map(|secret| (secret.id(), secret)));
        SecretBundleState {
            latest: find_latest(&secrets),
            secrets,
        }
    }

    /// Initialises secret bundle state from an encoded CBOR representation.
    pub(crate) fn try_from_bytes(bytes: &[u8]) -> Result<SecretBundleState, GroupSecretError> {
        Ok(decode_cbor(bytes)?)
    }

    /// Generates and returns a new secret.
    ///
    /// To prevent issues with invalid timestamps (too far in the future) of previous secrets, we
    /// force our newly generated secret to be the "latest" when needed.
    ///
    /// The secret is not yet inserted into the bundle, this needs to be done manually.
    pub fn generate(y: &SecretBundleState, rng: &Rng) -> Result<GroupSecret, GroupSecretError> {
        let mut secret = GroupSecret::from_rng(rng)?;

        let latest_timestamp = y.latest().map(|latest| latest.timestamp()).unwrap_or(0);
        if secret.timestamp() <= latest_timestamp {
            secret.set_timestamp(latest_timestamp + 1);
        }

        Ok(secret)
    }

    /// Inserts secret into bundle, ignoring duplicates.
    pub fn insert(mut y: SecretBundleState, secret: GroupSecret) -> SecretBundleState {
        y.secrets.insert(secret.id(), secret);
        y.latest = find_latest(&y.secrets);
        y
    }

    /// Removes secret from bundle.
    pub fn remove(
        mut y: SecretBundleState,
        id: &GroupSecretId,
    ) -> (SecretBundleState, Option<GroupSecret>) {
        let result = y.secrets.remove(id);
        y.latest = find_latest(&y.secrets);
        (y, result)
    }

    /// Merges one bundle with another, ignoring duplicates.
    pub fn extend(mut y: SecretBundleState, other: SecretBundleState) -> SecretBundleState {
        y.secrets.extend(other.secrets);
        y.latest = find_latest(&y.secrets);
        y
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

    use super::{GroupSecret, SecretBundle};

    #[test]
    fn group_secret_bundle() {
        let rng = Rng::from_seed([1; 32]);

        let secret = GroupSecret::from_rng(&rng).unwrap();

        let bundle_1 = SecretBundle::from_secrets(vec![secret.clone()]);
        let bundle_2 = SecretBundle::from_secrets(vec![secret.clone()]);
        assert_eq!(bundle_1.len(), 1);
        assert_eq!(bundle_2.len(), 1);

        assert_eq!(bundle_1.get(&secret.id()), bundle_2.get(&secret.id()));
        assert!(bundle_1.get(&secret.id()).is_some());
        assert!(bundle_1.contains(&secret.id()));

        let unknown_secret = GroupSecret::from_rng(&rng).unwrap();
        assert!(bundle_1.get(&unknown_secret.id()).is_none());
        assert!(!bundle_1.contains(&unknown_secret.id()));

        let secret_2 = GroupSecret::from_rng(&rng).unwrap();
        let bundle_2 = SecretBundle::insert(bundle_2, secret_2.clone());
        assert_eq!(bundle_2.len(), 2);

        let bundle_1 = SecretBundle::extend(bundle_1, bundle_2);
        assert_eq!(bundle_1.len(), 2);

        let (bundle_1, result) = SecretBundle::remove(bundle_1, &secret_2.id());
        assert_eq!(result, Some(secret_2));
        assert_eq!(bundle_1.len(), 1);
    }

    #[test]
    fn latest_secret() {
        let bundle = SecretBundle::init();
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
        let bundle = SecretBundle::insert(bundle, secret_1.clone());
        assert_eq!(bundle.len(), 1);
        assert_eq!(bundle.latest(), Some(&secret_1));

        // Inserted secret 2 is the "latest" as the higher hash wins when both timestamps are the
        // same.
        let bundle = SecretBundle::insert(bundle, secret_2.clone());
        assert_eq!(bundle.len(), 2);
        assert_eq!(bundle.latest(), Some(&secret_2));

        // Use a separate group to confirm that the order of insertion does not matter here.
        {
            let bundle_2 = SecretBundle::init();
            let bundle_2 = SecretBundle::insert(bundle_2, secret_2.clone());
            let bundle_2 = SecretBundle::insert(bundle_2, secret_1.clone());
            assert_eq!(bundle_2.latest(), Some(&secret_2));
        }

        // Inserted 3 is the latest.
        let bundle = SecretBundle::insert(bundle, secret_3.clone());
        assert_eq!(bundle.len(), 3);
        assert_eq!(bundle.latest(), Some(&secret_3));

        // Inserted 3 is still the latest.
        let bundle = SecretBundle::insert(bundle, secret_4.clone());
        assert_eq!(bundle.len(), 4);
        assert_eq!(bundle.latest(), Some(&secret_3));
    }

    #[test]
    fn serde() {
        let rng = Rng::from_seed([1; 32]);

        // Serialize & deserialize bundle.
        let bundle = SecretBundle::from_secrets(vec![
            GroupSecret::from_rng(&rng).unwrap(),
            GroupSecret::from_rng(&rng).unwrap(),
            GroupSecret::from_rng(&rng).unwrap(),
            GroupSecret::from_rng(&rng).unwrap(),
        ]);

        let bytes = bundle.to_bytes().unwrap();
        assert_eq!(bundle, SecretBundle::try_from_bytes(&bytes).unwrap());

        // Serialize & deserialize single secret.
        let secret = GroupSecret::from_rng(&rng).unwrap();
        let timestamp = secret.timestamp();

        let bytes = secret.to_bytes().unwrap();
        let secret_again = GroupSecret::try_from_bytes(&bytes).unwrap();
        assert_eq!(secret, secret_again);
        assert_eq!(timestamp, secret_again.timestamp());
    }

    #[test]
    fn generated_always_latest() {
        let rng = Rng::from_seed([1; 32]);

        let bundle = SecretBundle::init();

        let secret_0 = SecretBundle::generate(&bundle, &rng).unwrap();
        let bundle = SecretBundle::insert(bundle, secret_0.clone());
        assert_eq!(bundle.latest(), Some(&secret_0));

        let secret_1 = SecretBundle::generate(&bundle, &rng).unwrap();
        let bundle = SecretBundle::insert(bundle, secret_1.clone());
        assert_eq!(bundle.latest(), Some(&secret_1));

        let secret_2 = SecretBundle::generate(&bundle, &rng).unwrap();
        let bundle = SecretBundle::insert(bundle, secret_2.clone());
        assert_eq!(bundle.latest(), Some(&secret_2));
    }
}
