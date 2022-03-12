// SPDX-License-Identifier: AGPL-3.0-or-later

use std::fmt::Display;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::hash::{Hash, HashError};
use crate::Validate;

/// The identifier of a document view.
///
/// Contains the hashes of the document graph tips which is all the information we need to reliably
/// reconstruct a specific version of a document.
///
/// ```text
/// The document with the following operation graph has the id "2fa.." and six different document
/// view ids, meaning that this document can be represented in six versions:
///
/// 1. ["2fa"]
/// 2. ["de8"]
/// 3. ["89c"]
/// 4. ["eff"]
/// 5. ["de8", "eff"]
/// 6. ["89c", "eff"]
///
/// [CREATE] (Hash: "2fa..") <-- [UPDATE] (Hash: "de8..") <-- [UPDATE] (Hash: "89c..")
///                         \
///                          \__ [UPDATE] (Hash: "eff..")
/// ```
#[derive(Debug, Eq, PartialEq, Clone, Serialize, Deserialize)]
pub struct DocumentViewId(Vec<Hash>);

impl DocumentViewId {
    /// Create a new document view id.
    pub fn new(graph_tips: Vec<Hash>) -> Self {
        Self(graph_tips)
    }

    /// Get the graph tip hashes of this view id.
    pub fn graph_tips(&self) -> &[Hash] {
        self.0.as_slice()
    }

    /// Returns a hash over the graph tips constituting this view id.
    pub fn hash(&self) -> Hash {
        let graph_tip_bytes = self
            .0
            .clone()
            .into_iter()
            .flat_map(|graph_tip| graph_tip.to_bytes())
            .collect();
        Hash::new_from_bytes(graph_tip_bytes).unwrap()
    }
}

impl Display for DocumentViewId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.hash().as_str())
    }
}

impl Validate for DocumentViewId {
    type Error = HashError;

    fn validate(&self) -> Result<(), Self::Error> {
        for hash in &self.0 {
            hash.validate()?;
        }

        Ok(())
    }
}

impl IntoIterator for DocumentViewId {
    type Item = Hash;

    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

/// Convenience method converting a single hash into a document view id.
///
/// Converts a `Hash` instance into a `DocumentViewId`, assuming that this document view only
/// consists of one graph tip hash.
impl From<Hash> for DocumentViewId {
    fn from(hash: Hash) -> Self {
        Self::new(vec![hash])
    }
}

/// Convenience method converting a hash string into a document view id.
///
/// Converts a hash string into a `DocumentViewId`, assuming that this document view only consists
/// of one graph tip hash.
impl FromStr for DocumentViewId {
    type Err = HashError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self::new(vec![Hash::new(s)?]))
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use crate::hash::Hash;
    use crate::test_utils::fixtures::random_hash;
    use crate::Validate;

    use super::DocumentViewId;

    #[rstest]
    fn conversion(#[from(random_hash)] hash: Hash) {
        // Converts any string to `DocumentViewId`
        let hash_str = "0020d3235c8fe6f58608200851b83cd8482808eb81e4c6b4b17805bba57da9f16e79";
        let document_id: DocumentViewId = hash_str.parse().unwrap();
        assert_eq!(
            document_id,
            DocumentViewId::new(vec![Hash::new(hash_str).unwrap()])
        );

        // Converts any `Hash` to `DocumentViewId`
        let document_id = DocumentViewId::from(hash.clone());
        assert_eq!(document_id, DocumentViewId::new(vec![hash]));

        // Fails when string is not a hash
        assert!("This is not a hash".parse::<DocumentViewId>().is_err());
    }

    #[rstest]
    fn iterates(#[from(random_hash)] hash_1: Hash, #[from(random_hash)] hash_2: Hash) {
        let document_view_id = DocumentViewId::new(vec![hash_1, hash_2]);

        for hash in document_view_id {
            assert!(hash.validate().is_ok());
        }
    }

    #[rstest]
    fn hashes(#[from(random_hash)] hash_1: Hash, #[from(random_hash)] hash_2: Hash) {
        let document_view_id = DocumentViewId::new(vec![hash_1, hash_2]);

        assert_eq!(document_view_id.hash().as_str(), "");
    }
}
