// SPDX-License-Identifier: AGPL-3.0-or-later

use std::convert::TryFrom;
use std::fmt::{self, Display, Write};
use std::hash::Hash as StdHash;
use std::slice::Iter;
use std::str::FromStr;

use serde::de::Visitor;
use serde::{Deserialize, Deserializer, Serialize};

use crate::document::error::DocumentViewIdError;
use crate::hash::{Hash, HashId};
use crate::operation::error::OperationIdError;
use crate::operation::OperationId;
use crate::{Human, Validate};

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
#[derive(Clone, Debug, Ord, PartialOrd, Eq, PartialEq, StdHash, Serialize)]
pub struct DocumentViewId(Vec<OperationId>);

impl DocumentViewId {
    /// Create a new document view id.
    ///
    /// The given operation ids will automatically be sorted and de-duplicated.
    pub fn new(graph_tips: &[OperationId]) -> Self {
        let mut graph_tips = graph_tips.to_owned();

        // Make sure all operations are sorted
        graph_tips.sort();

        // Make sure all operations are de-duplicated
        graph_tips.dedup();

        Self(graph_tips)
    }

    /// Create a new document view id from an untrusted source.
    ///
    /// This does _not_ automatically sort and de-duplicate the given operation ids but validates
    /// them instead and returns an error if invalid.
    ///
    /// Use this method internally to ensure that document view ids from untrusted sources are
    /// checked.
    pub(crate) fn from_untrusted(
        graph_tips: Vec<OperationId>,
    ) -> Result<Self, DocumentViewIdError> {
        // Create document view id with potentially invalid data
        let document_view_id = Self(graph_tips);

        // Make sure its sorted and does not contain any duplicates (#OP3)
        document_view_id.validate()?;

        Ok(document_view_id)
    }

    /// Get the operation ids of this view id.
    pub fn graph_tips(&self) -> &[OperationId] {
        self.0.as_slice()
    }

    /// Get the operation ids of this view id.
    pub fn iter(&self) -> Iter<OperationId> {
        self.graph_tips().iter()
    }
}

impl Validate for DocumentViewId {
    type Error = DocumentViewIdError;

    /// Checks document view id against canonic format.
    ///
    /// This verifies if the document view id is not empty and constituting operation ids are
    /// sorted, do not contain any duplicates and represent valid hashes (#OP3).
    fn validate(&self) -> Result<(), Self::Error> {
        // Check if at least one operation id is given
        if self.0.is_empty() {
            return Err(DocumentViewIdError::ZeroOperationIds);
        };

        let mut prev_operation_id: Option<&OperationId> = None;

        for operation_id in &self.0 {
            // Check if the given operation ids are correctly formatted
            operation_id.validate()?;

            // Check if it is sorted, this indirectly also checks against duplicates
            if let Some(prev) = prev_operation_id {
                if prev >= operation_id {
                    return Err(DocumentViewIdError::UnsortedOperationIds);
                }
            }

            prev_operation_id = Some(operation_id);
        }

        Ok(())
    }
}

impl Display for DocumentViewId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (i, operation_id) in self.iter().enumerate() {
            let separator = if i == 0 { "" } else { "_" };
            let _ = write!(f, "{}{}", &separator, operation_id.as_str());
        }

        Ok(())
    }
}

impl Human for DocumentViewId {
    fn display(&self) -> String {
        let mut result = String::new();
        let offset = yasmf_hash::MAX_YAMF_HASH_SIZE * 2 - 6;

        for (i, operation_id) in self.iter().enumerate() {
            let separator = if i == 0 { "" } else { "_" };
            write!(result, "{}{}", &separator, &operation_id.as_str()[offset..]).unwrap();
        }

        result
    }
}

impl<'de> Deserialize<'de> for DocumentViewId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct DocumentViewIdVisitor;

        impl<'de> Visitor<'de> for DocumentViewIdVisitor {
            type Value = DocumentViewId;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("schema id as string")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                DocumentViewId::from_str(value).map_err(serde::de::Error::custom)
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::SeqAccess<'de>,
            {
                let mut operation_ids = Vec::new();

                while let Some(operation_id) = seq.next_element::<OperationId>()? {
                    operation_ids.push(operation_id);
                }

                let view_id = DocumentViewId::from_untrusted(operation_ids)
                    .map_err(serde::de::Error::custom)?;

                Ok(view_id)
            }
        }

        let view_id = deserializer.deserialize_any(DocumentViewIdVisitor)?;

        Ok(view_id)
    }
}

impl TryFrom<&[String]> for DocumentViewId {
    type Error = DocumentViewIdError;

    fn try_from(str_list: &[String]) -> Result<Self, Self::Error> {
        let operation_ids: Result<Vec<OperationId>, OperationIdError> = str_list
            .iter()
            .map(|operation_id_str| operation_id_str.parse::<OperationId>())
            .collect();

        Self::from_untrusted(operation_ids?)
    }
}

impl<T: HashId> TryFrom<&[T]> for DocumentViewId {
    type Error = DocumentViewIdError;

    fn try_from(str_list: &[T]) -> Result<Self, Self::Error> {
        let operation_ids: Vec<OperationId> = str_list
            .iter()
            .map(|hash_id| hash_id.as_hash().clone().into())
            .collect();

        Self::from_untrusted(operation_ids)
    }
}

impl TryFrom<&[Hash]> for DocumentViewId {
    type Error = DocumentViewIdError;

    fn try_from(str_list: &[Hash]) -> Result<Self, Self::Error> {
        let operation_ids: Vec<OperationId> =
            str_list.iter().map(|hash| hash.clone().into()).collect();

        Self::from_untrusted(operation_ids)
    }
}

impl From<DocumentViewId> for Vec<Hash> {
    fn from(value: DocumentViewId) -> Self {
        value.iter().map(HashId::as_hash).cloned().collect()
    }
}

/// Convenience method converting a single [`OperationId`] into a document view id.
///
/// Converts an `OperationId` instance into a `DocumentViewId`, assuming that this document view
/// only consists of one graph tip hash.
impl From<OperationId> for DocumentViewId {
    fn from(operation_id: OperationId) -> Self {
        Self(vec![operation_id])
    }
}

/// Convenience method converting a single hash into a document view id.
///
/// Converts a `Hash` instance into a `DocumentViewId`, assuming that this document view only
/// consists of one graph tip hash.
impl From<Hash> for DocumentViewId {
    fn from(hash: Hash) -> Self {
        Self(vec![hash.into()])
    }
}

/// Convenience method converting a hash string into a document view id.
///
/// Converts a string formatted document view id into a `DocumentViewId`. Expects multi-hash ids to
/// be hash strings separated by an `_` character.
impl FromStr for DocumentViewId {
    type Err = DocumentViewIdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut operations: Vec<OperationId> = Vec::new();

        s.split('_')
            .try_for_each::<_, Result<(), Self::Err>>(|hash_str| {
                let operation_id = OperationId::from_str(hash_str)?;
                operations.push(operation_id);
                Ok(())
            })?;

        Self::from_untrusted(operations)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash as StdHash, Hasher};
    use std::str::FromStr;

    use rstest::rstest;
    use serde::{Deserialize, Serialize};

    use crate::hash::Hash;
    use crate::operation::OperationId;
    use crate::serde::{hex_string_to_bytes, serialize_from};
    use crate::test_utils::constants::HASH;
    use crate::test_utils::fixtures::random_hash;
    use crate::test_utils::fixtures::{document_view_id, random_operation_id};
    use crate::{Human, Validate};

    use super::DocumentViewId;

    #[rstest]
    fn constructor_converts_to_canonic_format() {
        let operation_id_1: OperationId =
            "00201413ae916e6745ab715c1f5ab49c47d6773c3c0febd970ecf1039beed203b472"
                .parse()
                .unwrap();
        let operation_id_2: OperationId =
            "0020266fe901ea7d3efa983f12d145089d29480064b0da7393a8c0779af7488c7f0d"
                .parse()
                .unwrap();
        let operation_id_3: OperationId =
            "0020387b96cfdc7ac155eff0a9941400dee4a21e7cf18dcccefbf0a46a7c0138bbf5"
                .parse()
                .unwrap();
        let operation_id_4: OperationId =
            "002047e8d17a2edb41621beec8c710ee71a1b2ea81d356f05cd466526b269a7b2493"
                .parse()
                .unwrap();

        // Everything is in order
        let document_view_id_1 = DocumentViewId::new(&[
            operation_id_1.clone(),
            operation_id_2.clone(),
            operation_id_3.clone(),
            operation_id_4.clone(),
        ]);
        assert!(document_view_id_1.validate().is_ok());

        // Unordered operations will be sorted and deduplicated
        let document_view_id_2 = DocumentViewId::new(&[
            operation_id_3.clone(),
            operation_id_3.clone(),
            operation_id_2.clone(),
            operation_id_4.clone(),
            operation_id_1.clone(),
            operation_id_4.clone(),
        ]);
        assert!(document_view_id_2.validate().is_ok());
        assert_eq!(document_view_id_2.graph_tips().len(), 4);

        assert!(DocumentViewId::from_untrusted(vec![
            operation_id_3.clone(),
            operation_id_3,
            operation_id_2,
            operation_id_4.clone(),
            operation_id_1,
            operation_id_4,
        ])
        .is_err());
    }

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
        let document_id: DocumentViewId = OperationId::new(&hash).into();
        assert_eq!(document_id, DocumentViewId::new(&[hash.into()]));

        // Fails when string is not a hash
        assert!("This is not a hash".parse::<DocumentViewId>().is_err());
    }

    #[rstest]
    fn iterates(document_view_id: DocumentViewId) {
        for hash in document_view_id.graph_tips() {
            assert!(hash.validate().is_ok());
        }
    }

    #[test]
    fn string_representation() {
        let document_view_id = HASH.parse::<DocumentViewId>().unwrap();

        assert_eq!(
            document_view_id.to_string(),
            "0020b177ec1bf26dfb3b7010d473e6d44713b29b765b99c6e60ecbfae742de496543"
        );

        assert_eq!(
            format!("{}", document_view_id),
            "0020b177ec1bf26dfb3b7010d473e6d44713b29b765b99c6e60ecbfae742de496543"
        );

        let operation_1 = "0020b177ec1bf26dfb3b7010d473e6d44713b29b765b99c6e60ecbfae742de496543"
            .parse::<OperationId>()
            .unwrap();
        let operation_2 = "0020d3235c8fe6f58608200851b83cd8482808eb81e4c6b4b17805bba57da9f16e79"
            .parse::<OperationId>()
            .unwrap();

        let document_view_id = DocumentViewId::new(&[operation_1, operation_2]);
        assert_eq!(document_view_id.to_string(), "0020b177ec1bf26dfb3b7010d473e6d44713b29b765b99c6e60ecbfae742de496543_0020d3235c8fe6f58608200851b83cd8482808eb81e4c6b4b17805bba57da9f16e79");
        assert_eq!("0020b177ec1bf26dfb3b7010d473e6d44713b29b765b99c6e60ecbfae742de496543_0020d3235c8fe6f58608200851b83cd8482808eb81e4c6b4b17805bba57da9f16e79".parse::<DocumentViewId>().unwrap(), document_view_id);
    }

    #[test]
    fn short_representation() {
        let operation_1 = "0020b177ec1bf26dfb3b7010d473e6d44713b29b765b99c6e60ecbfae742de496543"
            .parse::<OperationId>()
            .unwrap();
        let operation_2 = "0020d3235c8fe6f58608200851b83cd8482808eb81e4c6b4b17805bba57da9f16e79"
            .parse::<OperationId>()
            .unwrap();

        let view_id_unmerged = DocumentViewId::new(&[operation_1, operation_2]);
        assert_eq!(view_id_unmerged.display(), "496543_f16e79");
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
    fn hash_equality(
        #[from(random_operation_id)] operation_id_1: OperationId,
        #[from(random_operation_id)] operation_id_2: OperationId,
    ) {
        let mut hasher_1 = DefaultHasher::default();
        let mut hasher_2 = DefaultHasher::default();
        let view_id_1 = DocumentViewId::new(&[operation_id_1.clone(), operation_id_2.clone()]);
        let view_id_2 = DocumentViewId::new(&[operation_id_2, operation_id_1]);
        view_id_1.hash(&mut hasher_1);
        view_id_2.hash(&mut hasher_2);
        assert_eq!(hasher_1.finish(), hasher_2.finish());
    }

    #[rstest]
    fn deserialize_unsorted_view_id(
        #[from(random_operation_id)] operation_id_1: OperationId,
        #[from(random_operation_id)] operation_id_2: OperationId,
    ) {
        // Unsorted operation ids in document view id array:
        let unsorted_hashes = [
            hex_string_to_bytes(
                "0020c13cdc58dfc6f4ebd32992ff089db79980363144bdb2743693a019636fa72ec8",
            ),
            hex_string_to_bytes(
                "00202dce4b32cd35d61cf54634b93a526df333c5ed3d93230c2f026f8d1ecabc0cd7",
            ),
        ];
        let mut cbor_bytes = Vec::new();
        ciborium::ser::into_writer(&unsorted_hashes, &mut cbor_bytes).unwrap();

        // Construct document view id by deserialising CBOR data
        let result: Result<DocumentViewId, ciborium::de::Error<std::io::Error>> =
            ciborium::de::from_reader(&cbor_bytes[..]);

        let expected_result = ciborium::de::Error::<std::io::Error>::Semantic(
            None,
            "expected sorted operation ids in document view id".to_string(),
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
            hex_string_to_bytes(
                "0020c13cdc58dfc6f4ebd32992ff089db79980363144bdb2743693a019636fa72ec8",
            ),
            hex_string_to_bytes("2dce4b32cd35d61cf54634b93a526df333c5ed3d93230c2f026f8d1ecabc0cd7"),
        ];
        let mut cbor_bytes = Vec::new();
        ciborium::ser::into_writer(&invalid_hashes, &mut cbor_bytes).unwrap();
        let invalid_id_encoded = hex::encode(cbor_bytes);

        // Construct document view id by deserialising CBOR data
        let result: Result<DocumentViewId, ciborium::de::Error<std::io::Error>> =
            ciborium::de::from_reader(&hex::decode(invalid_id_encoded).unwrap()[..]);

        let expected_result = ciborium::de::Error::<std::io::Error>::Semantic(
            None,
            "invalid hash length 32 bytes, expected 34 bytes".to_string(),
        );

        assert_eq!(result.unwrap_err().to_string(), expected_result.to_string());
    }

    #[test]
    fn deserialize_human_readable() {
        let hash_str = "0020cfb0fa37f36d082faad3886a9ffbcc2813b7afe90f0609a556d425f1a76ec805_0020cfb0fa37f36d082faad3886a9ffbcc2813b7afe90f0609a556d425f1a76ec808";

        #[derive(Deserialize, Serialize, Debug, PartialEq)]
        struct Test {
            document_view_id: DocumentViewId,
        }

        // Deserialize from human-readable (hex-encoded) JSON string
        let json = format!(
            r#"
            {{
                "document_view_id": "{hash_str}"
            }}
        "#
        );

        let result: Test = serde_json::from_str(&json).unwrap();
        assert_eq!(
            Test {
                document_view_id: DocumentViewId::from_str(hash_str).unwrap(),
            },
            result
        );

        // Serialize into non human-readable CBOR format (operation ids are encoded as bytes)
        let bytes = serialize_from(result);
        assert_eq!(
            bytes,
            [
                // {"document_view_id":
                // [h'0020CFB0FA37F36D082FAAD3886A9FFBCC2813B7AFE90F0609A556D425F1A76EC805',
                // h'0020CFB0FA37F36D082FAAD3886A9FFBCC2813B7AFE90F0609A556D425F1A76EC808']}
                161, 112, 100, 111, 99, 117, 109, 101, 110, 116, 95, 118, 105, 101, 119, 95, 105,
                100, 130, 88, 34, 0, 32, 207, 176, 250, 55, 243, 109, 8, 47, 170, 211, 136, 106,
                159, 251, 204, 40, 19, 183, 175, 233, 15, 6, 9, 165, 86, 212, 37, 241, 167, 110,
                200, 5, 88, 34, 0, 32, 207, 176, 250, 55, 243, 109, 8, 47, 170, 211, 136, 106, 159,
                251, 204, 40, 19, 183, 175, 233, 15, 6, 9, 165, 86, 212, 37, 241, 167, 110, 200, 8
            ]
        )
    }
}
