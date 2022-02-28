// SPDX-License-Identifier: AGPL-3.0-or-later

//! Types and methods for deriving and maintaining materialised documents.
use std::collections::btree_map::Iter as BTreeMapIter;
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::document::DocumentId;
use crate::hash::{Hash, HashError};
use crate::operation::OperationValue;
use crate::Validate;

/// The identifier of a document view.
///
/// Contains the hashes of the document graph tips which is all the information we need to reliably
/// recreate the document at this certain point in time.
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

type FieldKey = String;

/// The materialised view of a `Document`. It's fields match the documents schema definition.
///
/// `DocumentViews` are immutable versions of a `Document`. They represent a document at a certain
/// point in time.
#[derive(Debug, PartialEq, Clone)]
pub struct DocumentView {
    /// Identifier of this document view.
    pub(crate) id: DocumentViewId,

    /// Identifier of the document this view is derived from.
    pub(crate) document_id: DocumentId,

    /// Materialized data held by this document view.
    pub(crate) view: BTreeMap<FieldKey, OperationValue>,
}

impl DocumentView {
    /// Construct a document view.
    ///
    /// Requires the DocumentId, DocumentViewId and field values to be calculated seperately and
    /// then passed in during construction.
    pub fn new(
        id: DocumentViewId,
        document_id: DocumentId,
        view: BTreeMap<FieldKey, OperationValue>,
    ) -> Self {
        Self {
            id,
            document_id,
            view,
        }
    }

    /// Get the id of this document view.
    pub fn id(&self) -> &DocumentViewId {
        &self.id
    }

    /// Get the id of this document.
    pub fn document_id(&self) -> &DocumentId {
        &self.document_id
    }

    /// Get a single value from this instance by it's key.
    pub fn get(&self, key: &str) -> Option<&OperationValue> {
        self.view.get(key)
    }

    /// Returns a vector containing the keys of this instance.
    pub fn keys(&self) -> Vec<String> {
        self.view.clone().into_keys().collect::<Vec<FieldKey>>()
    }

    /// Returns an iterator of existing instance fields.
    pub fn iter(&self) -> BTreeMapIter<FieldKey, OperationValue> {
        self.view.iter()
    }

    /// Returns the number of fields on this instance.
    pub fn len(&self) -> usize {
        self.view.len()
    }

    /// Returns true if the instance is empty, otherwise false.
    pub fn is_empty(&self) -> bool {
        self.view.is_empty()
    }
}
#[cfg(test)]
mod tests {
    use rstest::rstest;

    use crate::document::{reduce, DocumentId};
    use crate::hash::Hash;
    use crate::operation::{OperationValue, Relation};
    use crate::schema::SchemaId;
    use crate::test_utils::fixtures::{
        create_operation, delete_operation, fields, random_document_id, random_hash, schema,
        update_operation,
    };

    use super::{DocumentView, DocumentViewId};

    #[rstest]
    fn gets_the_right_values(
        schema: SchemaId,
        #[from(random_hash)] prev_op_hash: Hash,
        #[from(random_document_id)] document_id: DocumentId,
        #[from(random_document_id)] profile_picture_id: DocumentId,
        #[from(random_hash)] view_id: Hash,
    ) {
        let document_view_id = DocumentViewId::new(vec![view_id]);

        let relation = Relation::new(profile_picture_id);

        let create_operation = create_operation(
            schema.clone(),
            fields(vec![
                ("username", OperationValue::Text("bubu".to_owned())),
                ("height", OperationValue::Float(3.5)),
                ("age", OperationValue::Integer(28)),
                ("is_admin", OperationValue::Boolean(false)),
                (
                    "profile_picture",
                    OperationValue::Relation(relation.clone()),
                ),
            ]),
        );

        // Reduce a single CREATE `Operation`
        let (view, is_edited, is_deleted) = reduce(&[create_operation.clone()]);

        let document_view = DocumentView::new(document_view_id.clone(), document_id.clone(), view);

        assert_eq!(
            document_view.keys(),
            vec!["age", "height", "is_admin", "profile_picture", "username"]
        );
        assert!(!document_view.is_empty());
        assert_eq!(document_view.len(), 5);
        assert_eq!(
            document_view.get("username").unwrap(),
            &OperationValue::Text("bubu".to_owned())
        );
        assert_eq!(
            document_view.get("height").unwrap(),
            &OperationValue::Float(3.5)
        );
        assert_eq!(
            document_view.get("age").unwrap(),
            &OperationValue::Integer(28)
        );
        assert_eq!(
            document_view.get("is_admin").unwrap(),
            &OperationValue::Boolean(false)
        );
        assert_eq!(
            document_view.get("profile_picture").unwrap(),
            &OperationValue::Relation(relation)
        );
        assert!(!is_edited);
        assert!(!is_deleted);

        let update_operation = update_operation(
            schema.clone(),
            vec![prev_op_hash.clone()],
            fields(vec![
                ("age", OperationValue::Integer(29)),
                ("is_admin", OperationValue::Boolean(true)),
            ]),
        );

        // Reduce again now with an UPDATE operation as well
        let (view, is_edited, is_deleted) =
            reduce(&[create_operation.clone(), update_operation.clone()]);

        let document_view = DocumentView::new(document_view_id, document_id, view);

        assert_eq!(
            document_view.get("age").unwrap(),
            &OperationValue::Integer(29)
        );
        assert_eq!(
            document_view.get("is_admin").unwrap(),
            &OperationValue::Boolean(true)
        );
        assert!(is_edited);
        assert!(!is_deleted);

        let delete_operation = delete_operation(schema, vec![prev_op_hash]);

        // Reduce again now with a DELETE operation as well
        let (_document_view, is_edited, is_deleted) =
            reduce(&[create_operation, update_operation, delete_operation]);

        assert!(is_edited);
        assert!(is_deleted);
    }
}
