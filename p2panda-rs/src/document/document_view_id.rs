// SPDX-License-Identifier: AGPL-3.0-or-later

use std::fmt::Display;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::hash::Hash;
use crate::operation::OperationId;
use crate::Validate;

use super::error::DocumentViewIdError;

/// The identifier of a document view.
///
/// Contains the operation ids of the document graph tips, which is all the information we need
/// to reliably reconstruct a specific version of a document.
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
pub struct DocumentViewId(Vec<OperationId>);

impl DocumentViewId {
    /// Create a new document view id.
    pub fn new(graph_tips: &[OperationId]) -> Self {
        let mut graph_tips_mut = graph_tips.to_owned();
        graph_tips_mut.sort();
        Self(graph_tips_mut)
    }

    /// Get the graph tip ids of this view id.
    pub fn graph_tips(&self) -> &[OperationId] {
        self.0.as_slice()
    }

    /// Returns a hash over the graph tips constituting this view id.
    ///
    /// Use this as a unique identifier for a document if you need a value with a limited size. The
    /// document view id itself grows with the number of graph tips that the document has, which
    /// may not be desirable for an identifier.
    ///
    /// Keep in mind that when you refer to document views with this hash value it will not be
    /// possible to recover the document view id from it.
    pub fn hash(&self) -> Hash {
        let graph_tip_bytes = self
            .0
            .clone()
            .into_iter()
            .flat_map(|graph_tip| graph_tip.as_hash().to_bytes())
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
    type Error = DocumentViewIdError;

    fn validate(&self) -> Result<(), Self::Error> {
        let is_sorted = self
            .0
            .windows(2)
            .all(|operation_ids| operation_ids[0] <= operation_ids[1]);
        if !is_sorted {
            return Err(DocumentViewIdError::UnsortedOperationIds);
        }

        for hash in &self.0 {
            hash.validate()?;
        }

        Ok(())
    }
}

impl IntoIterator for DocumentViewId {
    type Item = OperationId;

    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

/// Convenience method converting a single [`OperationId`] into a document view id.
///
/// Converts an `OperationId` instance into a `DocumentViewId`, assuming that this document view
/// only consists of one graph tip hash.
impl From<OperationId> for DocumentViewId {
    fn from(operation_id: OperationId) -> Self {
        Self::new(&[operation_id])
    }
}

/// Convenience method converting a single hash into a document view id.
///
/// Converts a `Hash` instance into a `DocumentViewId`, assuming that this document view only
/// consists of one graph tip hash.
impl From<Hash> for DocumentViewId {
    fn from(hash: Hash) -> Self {
        Self::new(&[hash.into()])
    }
}

/// Convenience method converting a hash string into a document view id.
///
/// Converts a hash string into a `DocumentViewId`, assuming that this document view only consists
/// of one graph tip hash.
impl FromStr for DocumentViewId {
    type Err = DocumentViewIdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self::new(&[Hash::new(s)?.into()]))
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use crate::hash::Hash;
    use crate::operation::OperationId;
    use crate::test_utils::fixtures::{document_view_id, random_hash, random_operation_id};
    use crate::Validate;

    use super::DocumentViewId;

    #[rstest]
    fn conversion(#[from(random_hash)] hash: Hash) {
        // Converts a string to `DocumentViewId`
        let hash_str = "0020d3235c8fe6f58608200851b83cd8482808eb81e4c6b4b17805bba57da9f16e79";
        let document_id: DocumentViewId = hash_str.parse().unwrap();
        assert_eq!(
            document_id,
            DocumentViewId::new(&[hash_str.parse::<OperationId>().unwrap()])
        );

        // Converts a `Hash` to `DocumentViewId`
        let document_id: DocumentViewId = hash.clone().into();
        assert_eq!(document_id, DocumentViewId::new(&[hash.clone().into()]));

        // Converts an `OperationId` to `DocumentViewId`
        let document_id: DocumentViewId = OperationId::new(hash.clone()).into();
        assert_eq!(document_id, DocumentViewId::new(&[hash.into()]));

        // Fails when string is not a hash
        assert!("This is not a hash".parse::<DocumentViewId>().is_err());
    }

    #[rstest]
    fn iterates(document_view_id: DocumentViewId) {
        for hash in document_view_id {
            assert!(hash.validate().is_ok());
        }
    }

    #[rstest]
    fn equality(
        #[from(random_operation_id)] operation_id_1: OperationId,
        #[from(random_operation_id)] operation_id_2: OperationId,
    ) {
        let view_id_1 = DocumentViewId::new(&[operation_id_1.clone(), operation_id_2.clone()]);
        let view_id_2 = DocumentViewId::new(&[operation_id_2, operation_id_1]);
        assert_eq!(view_id_1, view_id_2);
    }

    #[rstest]
    fn document_view_hash(
        #[from(random_operation_id)] operation_id_1: OperationId,
        #[from(random_operation_id)] operation_id_2: OperationId,
    ) {
        let view_id_1 = DocumentViewId::new(&[operation_id_1.clone(), operation_id_2.clone()]);
        assert!(view_id_1.hash().validate().is_ok());

        let view_id_2 = DocumentViewId::new(&[operation_id_2, operation_id_1]);
        assert_eq!(view_id_1.hash(), view_id_2.hash());
    }

    #[test]
    fn deserialize_unsorted_view_id() {
        // Unsorted operation ids in document view id array:
        //
        // [
        //  "0020c13cdc58dfc6f4ebd32992ff089db79980363144bdb2743693a019636fa72ec8",
        //  "00202dce4b32cd35d61cf54634b93a526df333c5ed3d93230c2f026f8d1ecabc0cd7"
        // ]
        let unsorted_operation_ids = "827844303032306331336364633538646663366634656264333239393266663038396462373939383033363331343462646232373433363933613031393633366661373265633878443030323032646365346233326364333564363163663534363334623933613532366466333333633565643364393332333063326630323666386431656361626330636437";

        // Construct document view id by deserialising CBOR data
        let view_id_1: DocumentViewId =
            ciborium::de::from_reader(&hex::decode(unsorted_operation_ids).unwrap()[..]).unwrap();

        assert_eq!(
            format!("{}", view_id_1.validate().unwrap_err()),
            "Expected sorted operation ids in document view id"
        );
    }

    #[rstest]
    fn hashes(#[from(random_operation_id)] op_1: OperationId, #[from(random_operation_id)] op_2: OperationId) {
        let document_view_id = DocumentViewId::new(&[op_1, op_2]);

        assert_eq!(document_view_id.hash().as_str(), "");
    }
}
