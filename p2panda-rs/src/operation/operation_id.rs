// SPDX-License-Identifier: AGPL-3.0-or-later

use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::hash::{Hash, HashError};
use crate::Validate;

/// Uniquely identifies an [`Operation`].
///
/// An `OperationId` is the hash of the [`Entry`] with which an operation was published.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialOrd, PartialEq, Serialize, Deserialize)]
pub struct OperationId(Hash);

impl OperationId {
    /// Returns an `OperationId` given an entry's hash.
    pub fn new(entry_hash: Hash) -> Self {
        Self(entry_hash)
    }

    /// Extracts a string slice from the operation id's hash.
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    /// Access the inner [`Hash`] value of this operation id.
    pub fn as_hash(&self) -> &Hash {
        &self.0
    }
}

impl Validate for OperationId {
    type Error = HashError;

    fn validate(&self) -> Result<(), Self::Error> {
        self.0.validate()
    }
}

impl From<Hash> for OperationId {
    fn from(hash: Hash) -> Self {
        Self::new(hash)
    }
}

impl FromStr for OperationId {
    type Err = HashError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self::new(Hash::new(s)?))
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use crate::hash::Hash;
    use crate::test_utils::fixtures::random_hash;

    use super::OperationId;

    #[rstest]
    fn conversion(#[from(random_hash)] hash: Hash) {
        // Converts any string to `OperationId`
        let hash_str = "0020cfb0fa37f36d082faad3886a9ffbcc2813b7afe90f0609a556d425f1a76ec805";
        let operation_id: OperationId = hash_str.parse().unwrap();
        assert_eq!(operation_id, OperationId::new(Hash::new(hash_str).unwrap()));
        assert_eq!(operation_id.as_str(), hash_str);

        // Converts any `Hash` to `OperationId`
        let operation_id = OperationId::from(hash.clone());
        assert_eq!(operation_id, OperationId::new(hash));

        // Fails when string is not a hash
        assert!("This is not a hash".parse::<OperationId>().is_err());
    }
}
