// SPDX-License-Identifier: AGPL-3.0-or-later

use std::fmt::Display;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize};

use crate::hash::Hash;
use crate::operation::error::OperationIdError;
use crate::{Human, Validate};

/// Uniquely identifies an [`Operation`](crate::operation::Operation).
///
/// An `OperationId` is the hash of the [`Entry`](crate::entry::Entry) with which an operation was
/// published.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialOrd, PartialEq, Serialize)]
pub struct OperationId(Hash);

impl OperationId {
    /// Returns an `OperationId` given an entry's hash.
    pub fn new(entry_hash: &Hash) -> Self {
        Self(entry_hash.to_owned())
    }

    /// Extracts a string slice from the operation id's hash.
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    /// Access the inner [`crate::hash::Hash`] value of this operation id.
    pub fn as_hash(&self) -> &Hash {
        &self.0
    }
}

impl Validate for OperationId {
    type Error = OperationIdError;

    fn validate(&self) -> Result<(), Self::Error> {
        self.0.validate()?;
        Ok(())
    }
}

impl From<Hash> for OperationId {
    fn from(hash: Hash) -> Self {
        Self::new(&hash)
    }
}

impl FromStr for OperationId {
    type Err = OperationIdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Hash::new(s)?))
    }
}

impl Display for OperationId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl Human for OperationId {
    fn display(&self) -> String {
        let offset = yasmf_hash::MAX_YAMF_HASH_SIZE * 2 - 6;
        format!("<Operation {}>", &self.0.as_str()[offset..])
    }
}

impl<'de> Deserialize<'de> for OperationId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        // Deserialize into `Hash` struct
        let hash: Hash = Deserialize::deserialize(deserializer)?;

        // Check format
        hash.validate()
            .map_err(|err| serde::de::Error::custom(format!("invalid operation id, {}", err)))?;

        Ok(Self(hash))
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use ciborium::cbor;
    use rstest::rstest;

    use crate::hash::Hash;
    use crate::serde::{deserialize_into, serialize_from, serialize_value};
    use crate::test_utils::fixtures::random_hash;
    use crate::Human;

    use super::OperationId;

    #[test]
    fn from_str() {
        // Converts any string to `OperationId`
        let hash_str = "0020cfb0fa37f36d082faad3886a9ffbcc2813b7afe90f0609a556d425f1a76ec805";
        let operation_id: OperationId = hash_str.parse().unwrap();
        assert_eq!(
            operation_id,
            OperationId::new(&Hash::new(hash_str).unwrap())
        );

        // Fails when string is not a hash
        assert!("This is not a hash".parse::<OperationId>().is_err());
    }

    #[rstest]
    fn from_hash(#[from(random_hash)] hash: Hash) {
        // Converts any `Hash` to `OperationId`
        let operation_id = OperationId::from(hash.clone());
        assert_eq!(operation_id, OperationId::new(&hash));
    }

    #[test]
    fn string_representation() {
        let hash_str = "0020cfb0fa37f36d082faad3886a9ffbcc2813b7afe90f0609a556d425f1a76ec805";
        let operation_id = OperationId::new(&Hash::new(hash_str).unwrap());

        assert_eq!(operation_id.as_str(), hash_str);
        assert_eq!(operation_id.to_string(), hash_str);
        assert_eq!(format!("{}", operation_id), hash_str);
    }

    #[test]
    fn short_representation() {
        let hash_str = "0020cfb0fa37f36d082faad3886a9ffbcc2813b7afe90f0609a556d425f1a76ec805";
        let operation_id = OperationId::new(&Hash::new(hash_str).unwrap());

        assert_eq!(operation_id.display(), "<Operation 6ec805>");
    }

    #[test]
    fn serialize() {
        let bytes = serialize_from(
            OperationId::from_str(
                "0020cfb0fa37f36d082faad3886a9ffbcc2813b7afe90f0609a556d425f1a76ec805",
            )
            .unwrap(),
        );
        assert_eq!(
            bytes,
            vec![
                120, 68, 48, 48, 50, 48, 99, 102, 98, 48, 102, 97, 51, 55, 102, 51, 54, 100, 48,
                56, 50, 102, 97, 97, 100, 51, 56, 56, 54, 97, 57, 102, 102, 98, 99, 99, 50, 56, 49,
                51, 98, 55, 97, 102, 101, 57, 48, 102, 48, 54, 48, 57, 97, 53, 53, 54, 100, 52, 50,
                53, 102, 49, 97, 55, 54, 101, 99, 56, 48, 53
            ]
        );
    }

    #[test]
    fn deserialize() {
        let hash_str = "0020cfb0fa37f36d082faad3886a9ffbcc2813b7afe90f0609a556d425f1a76ec805";
        let operation_id: OperationId = deserialize_into(&serialize_value(cbor!(
            "0020cfb0fa37f36d082faad3886a9ffbcc2813b7afe90f0609a556d425f1a76ec805"
        )))
        .unwrap();
        assert_eq!(OperationId::from_str(hash_str).unwrap(), operation_id);

        // Invalid hashes
        let invalid_hash = deserialize_into::<OperationId>(&serialize_value(cbor!("1234")));
        assert!(invalid_hash.is_err());
        let empty_hash = deserialize_into::<OperationId>(&serialize_value(cbor!("")));
        assert!(empty_hash.is_err());
    }
}
