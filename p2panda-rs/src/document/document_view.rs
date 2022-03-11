// SPDX-License-Identifier: AGPL-3.0-or-later

//! Types and methods for deriving and maintaining materialised documents.
use std::collections::btree_map::Iter as BTreeMapIter;
use std::collections::BTreeMap;

use crate::document::DocumentViewId;
use crate::operation::OperationValue;

type FieldKey = String;

/// The materialised view of a `Document`. It's fields match the documents schema definition.
///
/// `DocumentViews` are immutable versions of a `Document`. They represent a document at a certain
/// point in time.
#[derive(Debug, PartialEq, Clone)]
pub struct DocumentView {
    /// Identifier of this document view.
    pub(crate) id: DocumentViewId,

    /// Materialized data held by this document view.
    pub(crate) view: BTreeMap<FieldKey, OperationValue>,
}

impl DocumentView {
    /// Construct a document view.
    ///
    /// Requires the DocumentViewId and field values to be calculated seperately and
    /// then passed in during construction.
    pub fn new(id: DocumentViewId, view: BTreeMap<FieldKey, OperationValue>) -> Self {
        Self { id, view }
    }

    /// Get the id of this document view.
    pub fn id(&self) -> &DocumentViewId {
        &self.id
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
    use rstest::{fixture, rstest};

    use crate::document::{reduce, DocumentId};
    use crate::hash::Hash;
    use crate::operation::{Operation, OperationValue, Relation};
    use crate::schema::SchemaId;
    use crate::test_utils::fixtures::{
        create_operation, document_id, fields, random_hash, schema, update_operation,
    };

    use super::{DocumentView, DocumentViewId};

    #[fixture]
    fn test_create_operation(schema: SchemaId, document_id: DocumentId) -> Operation {
        let relation = Relation::new(document_id);

        create_operation(
            schema,
            fields(vec![
                ("username", OperationValue::Text("bubu".to_owned())),
                ("height", OperationValue::Float(3.5)),
                ("age", OperationValue::Integer(28)),
                ("is_admin", OperationValue::Boolean(false)),
                ("profile_picture", OperationValue::Relation(relation)),
            ]),
        )
    }

    #[fixture]
    fn test_update_operation(
        schema: SchemaId,
        #[from(random_hash)] prev_op_hash: Hash,
    ) -> Operation {
        update_operation(
            schema,
            vec![prev_op_hash],
            fields(vec![
                ("age", OperationValue::Integer(29)),
                ("is_admin", OperationValue::Boolean(true)),
            ]),
        )
    }

    #[rstest]
    fn from_single_create_op(
        test_create_operation: Operation,
        #[from(random_hash)] view_id: Hash,
        #[from(document_id)] relation_id: DocumentId,
    ) {
        let document_view_id = DocumentViewId::new(vec![view_id]);
        let expected_relation = Relation::new(relation_id);

        // Reduce a single CREATE `Operation`
        let (view, is_edited, is_deleted) = reduce(&[test_create_operation]);

        let document_view = DocumentView::new(document_view_id, view);

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
            &OperationValue::Relation(expected_relation)
        );
        assert!(!is_edited);
        assert!(!is_deleted);
    }

    #[rstest]
    fn with_update_op(
        test_create_operation: Operation,
        test_update_operation: Operation,
        #[from(random_hash)] view_id: Hash,
    ) {
        let document_view_id = DocumentViewId::new(vec![view_id]);

        let (view, is_edited, is_deleted) = reduce(&[test_create_operation, test_update_operation]);

        let document_view = DocumentView::new(document_view_id, view);

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
    }
}
