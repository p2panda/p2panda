// SPDX-License-Identifier: AGPL-3.0-or-later

use std::fmt::Display;
use std::hash::Hash as StdHash;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::document::error::DocumentIdError;
use crate::hash::{Hash, HashId};
use crate::operation::OperationId;
use crate::{Human, Validate};

/// Identifier of a document.
///
/// Documents are formed by one or many operations which create, update or delete the regarding
/// document. The whole document is always identified by the [`OperationId`] of its initial
/// `CREATE` operation. This operation id is equivalent to the [`Hash`](crate::hash::Hash) of the
/// entry with which that operation was published.
///
/// ```text
/// The document with the following operation graph has the id "2fa..":
///
/// [CREATE] (Hash: "2fa..") <-- [UPDATE] (Hash: "de8..") <-- [UPDATE] (Hash: "89c..")
///                         \
///                          \__ [UPDATE] (Hash: "eff..")
/// ```
#[derive(Clone, Debug, StdHash, Ord, PartialOrd, Eq, PartialEq, Serialize, Deserialize)]
pub struct DocumentId(OperationId);

impl DocumentId {
    /// Creates a new instance of `DocumentId`.
    pub fn new(id: &OperationId) -> Self {
        Self(id.to_owned())
    }

    /// Returns the string representation of the document id.
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl HashId for DocumentId {
    /// Access the inner [`crate::hash::Hash`] value of this document id.
    fn as_hash(&self) -> &Hash {
        self.0.as_hash()
    }
}

impl Display for DocumentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0.as_str())
    }
}

impl Human for DocumentId {
    fn display(&self) -> String {
        let offset = yasmf_hash::MAX_YAMF_HASH_SIZE * 2 - 6;
        format!("<DocumentId {}>", &self.0.as_str()[offset..])
    }
}

impl Validate for DocumentId {
    type Error = DocumentIdError;

    fn validate(&self) -> Result<(), Self::Error> {
        self.0.validate()?;
        Ok(())
    }
}

impl From<Hash> for DocumentId {
    fn from(hash: Hash) -> Self {
        Self(hash.into())
    }
}

impl FromStr for DocumentId {
    type Err = DocumentIdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(s.parse::<OperationId>()?))
    }
}
// 
// impl<'de> Deserialize<'de> for DocumentId {
//     fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
//     where
//         D: serde::Deserializer<'de>,
//     {
//         // Deserialize into `OperationId` struct
//         let operation_id: OperationId = Deserialize::deserialize(deserializer)?;
// 
//         // Check format
//         operation_id
//             .validate()
//             .map_err(|err| serde::de::Error::custom(format!("invalid operation id, {}", err)))?;
// 
//         Ok(Self(operation_id))
//     }
// }

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use ciborium::cbor;
    use rstest::rstest;

    use crate::hash::Hash;
    use crate::operation::OperationId;
    use crate::serde::{deserialize_into, hex_string_to_bytes, serialize_from, serialize_value};
    use crate::test_utils::fixtures::random_hash;
    use crate::Human;

    use super::DocumentId;

    #[rstest]
    fn conversion(#[from(random_hash)] hash: Hash) {
        // Converts any string to `DocumentId`
        let hash_str = "0020cfb0fa37f36d082faad3886a9ffbcc2813b7afe90f0609a556d425f1a76ec805";
        let document_id: DocumentId = hash_str.parse().unwrap();
        assert_eq!(
            document_id,
            DocumentId::new(&hash_str.parse::<OperationId>().unwrap())
        );

        // Converts any `Hash` to `DocumentId`
        let document_id = DocumentId::from(hash.clone());
        assert_eq!(document_id, DocumentId::new(&hash.into()));

        // Fails when string is not a hash
        assert!("This is not a hash".parse::<DocumentId>().is_err());
    }

    #[test]
    fn string_representation() {
        let hash_str = "0020cfb0fa37f36d082faad3886a9ffbcc2813b7afe90f0609a556d425f1a76ec805";
        let document_id: DocumentId = hash_str.parse().unwrap();

        assert_eq!(document_id.to_string(), hash_str);
        assert_eq!(document_id.as_str(), hash_str);
        assert_eq!(format!("{}", document_id), hash_str);
    }

    #[test]
    fn short_representation() {
        let hash_str = "0020cfb0fa37f36d082faad3886a9ffbcc2813b7afe90f0609a556d425f1a76ec805";
        let document_id: DocumentId = hash_str.parse().unwrap();

        assert_eq!(document_id.display(), "<DocumentId 6ec805>");
    }

    #[test]
    fn serialize() {
        let bytes = serialize_from(
            DocumentId::from_str(
                "0020cfb0fa37f36d082faad3886a9ffbcc2813b7afe90f0609a556d425f1a76ec805",
            )
            .unwrap(),
        );
        assert_eq!(
            bytes,
            vec![
                88, 34, 0, 32, 207, 176, 250, 55, 243, 109, 8, 47, 170, 211, 136, 106, 159, 251,
                204, 40, 19, 183, 175, 233, 15, 6, 9, 165, 86, 212, 37, 241, 167, 110, 200, 5
            ]
        );
    }

    #[test]
    fn deserialize() {
        let hash_str = "0020cfb0fa37f36d082faad3886a9ffbcc2813b7afe90f0609a556d425f1a76ec805";
        let document_id: DocumentId =
            deserialize_into(&serialize_value(cbor!(hex_string_to_bytes(
                "0020cfb0fa37f36d082faad3886a9ffbcc2813b7afe90f0609a556d425f1a76ec805"
            ))))
            .unwrap();
        assert_eq!(DocumentId::from_str(hash_str).unwrap(), document_id);

        // Invalid hashes
        let invalid_hash = deserialize_into::<DocumentId>(&serialize_value(cbor!("1234")));
        assert!(invalid_hash.is_err());
        let empty_hash = deserialize_into::<DocumentId>(&serialize_value(cbor!("")));
        assert!(empty_hash.is_err());
    }
}
