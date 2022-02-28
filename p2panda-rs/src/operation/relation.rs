// SPDX-License-Identifier: AGPL-3.0-or-later

use serde::{Deserialize, Serialize};

use crate::document::{DocumentId, DocumentViewId};
use crate::hash::HashError;
use crate::Validate;

/// Field type representing references to other documents.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Relation(DocumentId);

impl Relation {
    /// Returns a new relation field.
    pub fn new(document: DocumentId) -> Self {
        Self(document)
    }

    /// Returns the relations document id.
    pub fn document_id(&self) -> &DocumentId {
        &self.0
    }
}

impl Validate for Relation {
    type Error = HashError;

    fn validate(&self) -> Result<(), Self::Error> {
        self.0.validate()
    }
}

/// Reference to the exact version of the document.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PinnedRelation(DocumentViewId);

impl PinnedRelation {
    /// Returns a new pinned relation field.
    pub fn new(document_view_id: DocumentViewId) -> Self {
        Self(document_view_id)
    }
}

impl Validate for PinnedRelation {
    type Error = HashError;

    fn validate(&self) -> Result<(), Self::Error> {
        self.0.validate()
    }
}

/// A `RelationList` can be used to reference multiple foreign documents from a document field.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RelationList(Vec<DocumentId>);

impl RelationList {
    /// Returns a new list of relations.
    pub fn new(relations: Vec<DocumentId>) -> Self {
        Self(relations)
    }
}

impl Validate for RelationList {
    type Error = HashError;

    fn validate(&self) -> Result<(), Self::Error> {
        for document_id in &self.0 {
            document_id.validate()?;
        }

        Ok(())
    }
}

/// A `PinnedRelationList` can be used to reference multiple documents views.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PinnedRelationList(Vec<DocumentViewId>);

impl PinnedRelationList {
    /// Returns a new list of pinned relations.
    pub fn new(relations: Vec<DocumentViewId>) -> Self {
        Self(relations)
    }
}

impl Validate for PinnedRelationList {
    type Error = HashError;

    fn validate(&self) -> Result<(), Self::Error> {
        for document_view in &self.0 {
            document_view.validate()?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use crate::document::{DocumentId, DocumentViewId};
    use crate::hash::Hash;
    use crate::operation::{OperationFields, OperationValue, OperationValueRelationList};
    use crate::test_utils::fixtures::{random_document_id, random_hash};

    use super::{PinnedRelationList, RelationList};

    #[rstest]
    fn relation_lists(
        #[from(random_document_id)] document_1: DocumentId,
        #[from(random_document_id)] document_2: DocumentId,
    ) {
        let relations = RelationList::new(vec![document_1, document_2]);

        let mut fields = OperationFields::new();
        assert!(fields
            .add(
                "locations",
                OperationValue::RelationList(OperationValueRelationList::Unpinned(relations))
            )
            .is_ok());
    }

    #[rstest]
    fn pinned_relation_lists(
        #[from(random_hash)] operation_id_1: Hash,
        #[from(random_hash)] operation_id_2: Hash,
        #[from(random_hash)] operation_id_3: Hash,
        #[from(random_hash)] operation_id_4: Hash,
        #[from(random_hash)] operation_id_5: Hash,
        #[from(random_hash)] operation_id_6: Hash,
    ) {
        let document_view_id_1 = DocumentViewId::new(vec![operation_id_1, operation_id_2]);
        let document_view_id_2 = DocumentViewId::new(vec![operation_id_3]);
        let document_view_id_3 =
            DocumentViewId::new(vec![operation_id_4, operation_id_5, operation_id_6]);

        let relations = PinnedRelationList::new(vec![
            document_view_id_1,
            document_view_id_2,
            document_view_id_3,
        ]);

        let mut fields = OperationFields::new();
        assert!(fields
            .add(
                "locations",
                OperationValue::RelationList(OperationValueRelationList::Pinned(relations))
            )
            .is_ok());
    }
}
