// SPDX-License-Identifier: AGPL-3.0-or-later

use std::str::FromStr;

#[cfg(test)]
use proptest_derive::Arbitrary;
use serde::{Deserialize, Serialize};

use crate::hash::{Hash, HashError};
use crate::Validate;

/// Identifier of a document.
///
/// Documents are formed by one or many operations which create, update or delete the regarding
/// document. The hash of the entry containing the CREATE operation is automatically also the
/// identifier of the whole document.
///
/// ```text
/// The document with the following operation graph has the id "2fa..":
///
/// [CREATE] (Hash: "2fa..") <-- [UPDATE] (Hash: "de8..") <-- [UPDATE] (Hash: "89c..")
///                         \
///                          \__ [UPDATE] (Hash: "eff..")
/// ```
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(test, derive(Arbitrary))]
pub struct DocumentId(Hash);

impl DocumentId {
    /// Creates a new instance of `DocumentId`.
    pub fn new(id: Hash) -> Self {
        Self(id)
    }

    /// Returns the string representation of the document id.
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl Validate for DocumentId {
    type Error = HashError;

    fn validate(&self) -> Result<(), Self::Error> {
        self.0.validate()
    }
}

impl From<Hash> for DocumentId {
    fn from(hash: Hash) -> Self {
        Self::new(hash)
    }
}

impl FromStr for DocumentId {
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

    use super::DocumentId;

    #[rstest]
    fn conversion(#[from(random_hash)] hash: Hash) {
        // Converts any string to `DocumentId`
        let hash_str = "0020cfb0fa37f36d082faad3886a9ffbcc2813b7afe90f0609a556d425f1a76ec805";
        let document_id: DocumentId = hash_str.parse().unwrap();
        assert_eq!(document_id, DocumentId::new(Hash::new(hash_str).unwrap()));
        assert_eq!(document_id.as_str(), hash_str);

        // Converts any `Hash` to `DocumentId`
        let document_id = DocumentId::from(hash.clone());
        assert_eq!(document_id, DocumentId::new(hash));

        // Fails when string is not a hash
        assert!("This is not a hash".parse::<DocumentId>().is_err());
    }
}
