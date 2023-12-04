// SPDX-License-Identifier: AGPL-3.0-or-later

use std::fmt::Display;

use crate::document::DocumentViewId;
use crate::hash_v2::{Hash, HashId};

/// Contains a hash over the sorted graph tips constituting this view id.
///
/// Use this as a unique identifier for a document if you need a value with a limited size. The
/// document view id itself grows with the number of graph tips that the document has, which may
/// not be desirable for an identifier.
///
/// Keep in mind that when you refer to document views with this hash value it will not be possible
/// to recover the document view id from it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocumentViewHash(Hash);

impl DocumentViewHash {
    /// Creates a new instance of `DocumentViewHash`.
    pub fn new(hash: Hash) -> Self {
        Self(hash)
    }

    /// Returns string representation of the document view hash as `&str`.
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl Display for DocumentViewHash {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl From<Hash> for DocumentViewHash {
    fn from(hash: Hash) -> Self {
        Self::new(hash)
    }
}

impl From<&DocumentViewId> for DocumentViewHash {
    fn from(document_view_id: &DocumentViewId) -> Self {
        let graph_tip_bytes: Vec<u8> = document_view_id
            .iter()
            .flat_map(|graph_tip| graph_tip.to_bytes())
            .collect();

        Self::new(Hash::new_from_bytes(&graph_tip_bytes))
    }
}
// 
// #[cfg(test)]
// mod tests {
//     use rstest::rstest;
// 
//     use crate::document::DocumentViewId;
//     use crate::hash_v2::Hash;
//     use crate::operation_v2::OperationId;
//     use crate::test_utils::fixtures::{random_hash, random_operation_id};
// 
//     use super::DocumentViewHash;
// 
//     #[rstest]
//     fn equality_after_conversion(
//         #[from(random_operation_id)] operation_id_1: OperationId,
//         #[from(random_operation_id)] operation_id_2: OperationId,
//     ) {
//         let view_id_1 = DocumentViewId::new(&[operation_id_1.clone(), operation_id_2.clone()]);
//         let view_hash_1 = DocumentViewHash::from(&view_id_1);
//         let view_id_2 = DocumentViewId::new(&[operation_id_2, operation_id_1]);
//         let view_hash_2 = DocumentViewHash::from(&view_id_2);
//         assert_eq!(view_hash_1, view_hash_2);
//     }
// 
//     #[rstest]
//     fn string_representation(#[from(random_hash)] hash: Hash) {
//         let document_view_hash = DocumentViewHash::new(hash.clone());
//         assert_eq!(hash.as_str(), document_view_hash.as_str());
//         assert_eq!(hash.as_str(), &document_view_hash.to_string());
//         assert_eq!(
//             format!("{}", document_view_hash),
//             document_view_hash.as_str()
//         )
//     }
// }
