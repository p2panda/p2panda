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
/// and deserialisation of a value will fail if this does not hold. This follows p2panda's
/// requirement that all serialised arrays must be sorted and leads to deterministic serialisation.
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
    pub fn sorted(&self) -> Vec<OperationId> {
        let mut graph_tips = self.0.clone();
        graph_tips.sort();
        graph_tips
    }

    /// Get a string representation of all graph tips in this view id.
    pub fn as_str(&self) -> String {
        let mut id_str = "".to_string();
        for (i, operation_id) in self.sorted().iter().enumerate() {
            let separator = if i == 0 { "" } else { "_" };
            id_str += format!("{}{}", separator, operation_id.as_hash().as_str()).as_str();
        }
        id_str
    }
}

impl fmt::Display for DocumentViewId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (i, operation_id) in self.0.clone().into_iter().enumerate() {
            let separator = if i == 0 { "" } else { "_" };
            write!(f, "{}{}", separator, operation_id.as_hash().short_str())?;
        }
        Ok(())
    }
}

impl std::hash::Hash for DocumentViewId {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.sorted().hash(state);
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
/// Converts a string formatted document view id into a `DocumentViewId`. Expects multi-hash ids to
/// be hash strings seperated by an `_` character.
impl FromStr for DocumentViewId {
    type Err = HashError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut operations: Vec<OperationId> = Vec::new();
        s.rsplit('_')
            .try_for_each::<_, Result<(), Self::Err>>(|hash_str| {
                let hash = Hash::new(hash_str)?;
                operations.push(hash.into());
                Ok(())
            })?;
        Ok(Self::new(&operations))
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use crate::hash::Hash;
    use crate::operation::OperationId;
    use crate::test_utils::constants::DEFAULT_HASH;
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

    #[test]
    fn debug_representation() {
        let document_view_id = DEFAULT_HASH.parse::<DocumentViewId>().unwrap();
        assert_eq!(format!("{}", document_view_id), "496543");

        let operation_1 = "0020b177ec1bf26dfb3b7010d473e6d44713b29b765b99c6e60ecbfae742de496543"
            .parse::<OperationId>()
            .unwrap();
        let operation_2 = "0020d3235c8fe6f58608200851b83cd8482808eb81e4c6b4b17805bba57da9f16e79"
            .parse::<OperationId>()
            .unwrap();
        let view_id_unmerged = DocumentViewId::new(&[operation_1, operation_2]);
        assert_eq!(format!("{}", view_id_unmerged), "496543_f16e79");
    }

    #[test]
    fn string_representation() {
        let document_view_id = DEFAULT_HASH.parse::<DocumentViewId>().unwrap();

        assert_eq!(
            document_view_id.as_str(),
            "0020b177ec1bf26dfb3b7010d473e6d44713b29b765b99c6e60ecbfae742de496543"
        );

        let operation_1 = "0020b177ec1bf26dfb3b7010d473e6d44713b29b765b99c6e60ecbfae742de496543"
            .parse::<OperationId>()
            .unwrap();
        let operation_2 = "0020d3235c8fe6f58608200851b83cd8482808eb81e4c6b4b17805bba57da9f16e79"
            .parse::<OperationId>()
            .unwrap();
        let document_view_id = DocumentViewId::new(&[operation_1, operation_2]);
        assert_eq!(document_view_id.as_str(), "0020b177ec1bf26dfb3b7010d473e6d44713b29b765b99c6e60ecbfae742de496543_0020d3235c8fe6f58608200851b83cd8482808eb81e4c6b4b17805bba57da9f16e79");
        assert_eq!("0020b177ec1bf26dfb3b7010d473e6d44713b29b765b99c6e60ecbfae742de496543_0020d3235c8fe6f58608200851b83cd8482808eb81e4c6b4b17805bba57da9f16e79".parse::<DocumentViewId>().unwrap(), document_view_id);
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
    fn deserialize_unsorted_view_id(
        #[from(random_operation_id)] operation_id_1: OperationId,
        #[from(random_operation_id)] operation_id_2: OperationId,
    ) {
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

        // However, unsorted values in an id are sorted during serialisation
        let mut reversed_ids = vec![operation_id_1, operation_id_2];
        reversed_ids.sort();
        reversed_ids.reverse();
        let view_id_unsorted = DocumentViewId::new(&reversed_ids);

        let mut cbor_bytes = Vec::new();
        ciborium::ser::into_writer(&view_id_unsorted, &mut cbor_bytes).unwrap();

        let result: Result<DocumentViewId, ciborium::de::Error<std::io::Error>> =
            ciborium::de::from_reader(&cbor_bytes[..]);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), view_id_unsorted);
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
    }
}
