// SPDX-License-Identifier: AGPL-3.0-or-later

use std::fmt;
use std::str::FromStr;

use serde::de::{SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};

use crate::hash::{Hash, HashError};
use crate::operation::OperationId;
use crate::Validate;

/// The identifier of a document view.
///
/// Contains the operation ids of the document graph tips, which is all the information we need
/// to reliably reconstruct a specific version of a document.
///
/// Document view ids are considered equal if they contain the same set of operation ids,
/// independent of their order. Serialised document view ids always contain sorted operation ids
/// and deserialisation of a value will fail if this does not hold.
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
#[derive(Clone, Debug, Eq)]
pub struct DocumentViewId(Vec<OperationId>);

impl DocumentViewId {
    /// Create a new document view id.
    pub fn new(graph_tips: &[OperationId]) -> Self {
        Self(graph_tips.to_vec())
    }

    /// Get the graph tip ids of this view id.
    pub fn graph_tips(&self) -> &[OperationId] {
        self.0.as_slice()
    }

    /// Get sorted graph tips for this view id.
    fn sorted(&self) -> Vec<OperationId> {
        let mut graph_tips = self.0.clone();
        graph_tips.sort();
        graph_tips
    }

    /// Returns a hash over the sorted graph tips constituting this view id.
    ///
    /// Use this as a unique identifier for a document if you need a value with a limited size. The
    /// document view id itself grows with the number of graph tips that the document has, which
    /// may not be desirable for an identifier.
    ///
    /// Keep in mind that when you refer to document views with this hash value it will not be
    /// possible to recover the document view id from it.
    pub fn hash(&self) -> Hash {
        let graph_tip_bytes = self
            .sorted()
            .into_iter()
            .flat_map(|graph_tip| graph_tip.as_hash().to_bytes())
            .collect();
        Hash::new_from_bytes(graph_tip_bytes).unwrap()
    }
}

impl PartialEq for DocumentViewId {
    fn eq(&self, other: &Self) -> bool {
        self.sorted() == other.sorted()
    }
}

impl Validate for DocumentViewId {
    type Error = HashError;

    /// Checks that constituting operation ids are sorted and represent valid hashes.
    fn validate(&self) -> Result<(), Self::Error> {
        for hash in &self.0 {
            hash.validate()?;
        }

        Ok(())
    }
}

impl Serialize for DocumentViewId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.sorted().serialize(serializer)
    }
}

struct DocumentViewIdVisitor;

impl<'de> Visitor<'de> for DocumentViewIdVisitor {
    type Value = DocumentViewId;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("sequence of operation id strings")
    }

    fn visit_seq<S>(self, mut seq: S) -> Result<Self::Value, S::Error>
    where
        S: SeqAccess<'de>,
    {
        let mut op_ids: Vec<OperationId> = Vec::new();
        let mut prev_id = None;

        while let Some(seq_value) = seq.next_element::<String>()? {
            // Try and parse next value as `OperationId`
            let operation_id = match seq_value.parse::<OperationId>() {
                Ok(operation_id) => operation_id,
                Err(hash_err) => {
                    return Err(serde::de::Error::custom(format!(
                        "Error parsing document view id at position {}: {}",
                        op_ids.len(),
                        hash_err
                    )))
                }
            };

            // Check that consecutive ids are sorted
            if prev_id.is_some() && prev_id.unwrap() > operation_id {
                return Err(serde::de::Error::custom(format!(
                    "Encountered unsorted value in document view id at position {}",
                    op_ids.len()
                )));
            }
            op_ids.push(operation_id.clone());
            prev_id = Some(operation_id);
        }

        Ok(DocumentViewId::new(&op_ids))
    }
}

impl<'de> Deserialize<'de> for DocumentViewId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_seq(DocumentViewIdVisitor)
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
    type Err = HashError;

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

    #[test]
    fn deserialize_unsorted_view_id() {
        // Unsorted operation ids in document view id array:
        let unsorted_hashes = [
            "0020c13cdc58dfc6f4ebd32992ff089db79980363144bdb2743693a019636fa72ec8",
            "00202dce4b32cd35d61cf54634b93a526df333c5ed3d93230c2f026f8d1ecabc0cd7",
        ];
        let mut cbor_bytes = Vec::new();
        ciborium::ser::into_writer(&unsorted_hashes, &mut cbor_bytes).unwrap();
        let unsorted_operation_ids = hex::encode(cbor_bytes);

        // Construct document view id by deserialising CBOR data
        let result: Result<DocumentViewId, ciborium::de::Error<std::io::Error>> =
            ciborium::de::from_reader(&hex::decode(unsorted_operation_ids).unwrap()[..]);

        let expected_result = ciborium::de::Error::<std::io::Error>::Semantic(
            None,
            "Encountered unsorted value in document view id at position 1".to_string(),
        );

        assert_eq!(result.unwrap_err().to_string(), expected_result.to_string());
    }

    #[test]
    fn deserialize_invalid_view_id() {
        // The second operation id is missing 4 characters
        let invalid_hashes = [
            "0020c13cdc58dfc6f4ebd32992ff089db79980363144bdb2743693a019636fa72ec8",
            "2dce4b32cd35d61cf54634b93a526df333c5ed3d93230c2f026f8d1ecabc0cd7",
        ];
        let mut cbor_bytes = Vec::new();
        ciborium::ser::into_writer(&invalid_hashes, &mut cbor_bytes).unwrap();
        let invalid_id_encoded = hex::encode(cbor_bytes);

        // Construct document view id by deserialising CBOR data
        let result: Result<DocumentViewId, ciborium::de::Error<std::io::Error>> =
            ciborium::de::from_reader(&hex::decode(invalid_id_encoded).unwrap()[..]);

        let expected_result = ciborium::de::Error::<std::io::Error>::Semantic(
            None,
            "Error parsing document view id at position 1: invalid hash length 32 bytes, expected 34 bytes".to_string()
        );

        assert_eq!(result.unwrap_err().to_string(), expected_result.to_string());

        let result_int = serde_json::from_str::<DocumentViewId>("5");
        let expected_err = "invalid type: integer `5`, expected sequence of operation id strings at line 1 column 1";
        assert_eq!(
            result_int.unwrap_err().to_string(),
            expected_err.to_string()
        );
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
}
