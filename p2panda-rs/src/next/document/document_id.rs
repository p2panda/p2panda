// SPDX-License-Identifier: AGPL-3.0-or-later

use std::fmt::Display;
use std::hash::Hash as StdHash;
use std::str::FromStr;

use crate::hash::{Hash, HashError};
use crate::next::operation::OperationId;
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
#[derive(Clone, Debug, StdHash, Ord, PartialOrd, Eq, PartialEq)]
pub struct DocumentId(OperationId);

impl DocumentId {
    /// Creates a new instance of `DocumentId`.
    pub fn new(id: OperationId) -> Self {
        Self(id)
    }

    /// Returns the string representation of the document id.
    pub fn as_str(&self) -> &str {
        self.0.as_str()
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

// @TODO: Evaluate if we still need this
impl Validate for DocumentId {
    type Error = HashError;

    fn validate(&self) -> Result<(), Self::Error> {
        self.0.validate()
    }
}

impl From<Hash> for DocumentId {
    fn from(hash: Hash) -> Self {
        Self::new(hash.into())
    }
}

impl FromStr for DocumentId {
    type Err = HashError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self::new(s.parse::<OperationId>()?))
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use crate::hash::Hash;
    use crate::next::operation::OperationId;
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
            DocumentId::new(hash_str.parse::<OperationId>().unwrap())
        );

        // Converts any `Hash` to `DocumentId`
        let document_id = DocumentId::from(hash.clone());
        assert_eq!(document_id, DocumentId::new(hash.into()));

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
}
