// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::document::DocumentViewId;
use crate::hash::Hash;

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

impl From<DocumentViewId> for DocumentViewHash {
    fn from(document_view_id: DocumentViewId) -> Self {
        let graph_tip_bytes = document_view_id
            .sorted()
            .into_iter()
            .flat_map(|graph_tip| graph_tip.as_hash().to_bytes())
            .collect();

        // Unwrap here as the content should be validated at this point
        Self(Hash::new_from_bytes(graph_tip_bytes).unwrap())
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use crate::document::DocumentViewId;
    use crate::operation::OperationId;
    use crate::test_utils::fixtures::random_operation_id;

    use super::DocumentViewHash;

    #[rstest]
    fn document_view_hash(
        #[from(random_operation_id)] operation_id_1: OperationId,
        #[from(random_operation_id)] operation_id_2: OperationId,
    ) {
        let view_id_1 = DocumentViewId::new(&[operation_id_1.clone(), operation_id_2.clone()]);
        let view_hash_1 = DocumentViewHash::from(view_id_1);
        let view_id_2 = DocumentViewId::new(&[operation_id_2, operation_id_1]);
        let view_hash_2 = DocumentViewHash::from(view_id_2);
        assert_eq!(view_hash_1, view_hash_2);
    }
}
