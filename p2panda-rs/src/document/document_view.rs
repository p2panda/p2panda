// SPDX-License-Identifier: AGPL-3.0-or-later

//! Types and methods for deriving and maintaining materialised documents.
use std::collections::btree_map::Iter as BTreeMapIter;
use std::fmt::Display;

use crate::document::DocumentViewId;
use crate::document::{DocumentViewFields, DocumentViewValue};

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
    pub(crate) fields: DocumentViewFields,
}

impl DocumentView {
    /// Construct a document view.
    ///
    /// Requires the DocumentViewId and field values to be calculated seperately and
    /// then passed in during construction.
    pub fn new(id: &DocumentViewId, fields: &DocumentViewFields) -> Self {
        Self {
            id: id.clone(),
            fields: fields.clone(),
        }
    }

    /// Get the id of this document view.
    pub fn id(&self) -> &DocumentViewId {
        &self.id
    }

    /// Get a single value from this instance by it's key.
    pub fn get(&self, key: &str) -> Option<&DocumentViewValue> {
        self.fields.get(key)
    }

    /// Returns a vector containing the keys of this instance.
    pub fn keys(&self) -> Vec<String> {
        self.fields.keys()
    }

    /// Returns an iterator of existing instance fields.
    pub fn iter(&self) -> BTreeMapIter<FieldKey, DocumentViewValue> {
        self.fields.iter()
    }

    /// Returns the number of fields on this instance.
    pub fn len(&self) -> usize {
        self.fields.len()
    }

    /// Returns true if the instance is empty, otherwise false.
    pub fn is_empty(&self) -> bool {
        self.fields.is_empty()
    }

    /// Returns the fields of this document view.
    pub fn fields(&self) -> &DocumentViewFields {
        &self.fields
    }
}

impl Display for DocumentView {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "<DocumentView {}>", self.id)
    }
}
#[cfg(test)]
mod tests {
    use rstest::{fixture, rstest};

    use crate::document::document_view_fields::DocumentViewValue;
    use crate::document::{reduce, DocumentId};
    use crate::identity::Author;
    use crate::operation::{OperationId, OperationValue, OperationWithMeta, Relation};
    use crate::schema::SchemaId;
    use crate::test_utils::fixtures::{
        create_operation, document_id, document_view_id, fields, public_key, random_operation_id,
        schema, update_operation,
    };

    use super::{DocumentView, DocumentViewId};

    #[fixture]
    fn test_create_operation(
        schema: SchemaId,
        #[from(random_operation_id)] operation_id: OperationId,
        document_id: DocumentId,
        public_key: Author,
    ) -> OperationWithMeta {
        let relation = Relation::new(document_id);

        let operation = create_operation(
            schema,
            fields(vec![
                ("username", OperationValue::Text("bubu".to_owned())),
                ("height", OperationValue::Float(3.5)),
                ("age", OperationValue::Integer(28)),
                ("is_admin", OperationValue::Boolean(false)),
                ("profile_picture", OperationValue::Relation(relation)),
            ]),
        );

        OperationWithMeta::new_test_operation(&operation_id, &public_key, &operation)
    }

    #[fixture]
    fn test_update_operation(
        #[from(random_operation_id)] operation_id: OperationId,
        #[from(random_operation_id)] prev_op_id: OperationId,
        schema: SchemaId,
        public_key: Author,
    ) -> OperationWithMeta {
        let operation = update_operation(
            schema,
            vec![prev_op_id],
            fields(vec![
                ("age", OperationValue::Integer(29)),
                ("is_admin", OperationValue::Boolean(true)),
            ]),
        );
        OperationWithMeta::new_test_operation(&operation_id, &public_key, &operation)
    }

    #[rstest]
    fn from_single_create_op(
        test_create_operation: OperationWithMeta,
        document_view_id: DocumentViewId,
        #[from(document_id)] relation_id: DocumentId,
    ) {
        let expected_relation = Relation::new(relation_id);

        // Reduce a single CREATE `Operation`
        let (view, is_edited, is_deleted) = reduce(&[test_create_operation.clone()]);

        let document_view = DocumentView::new(&document_view_id, &view.unwrap());

        assert_eq!(format!("{}", document_view), "<DocumentView 496543>");

        assert_eq!(
            document_view.keys(),
            vec!["age", "height", "is_admin", "profile_picture", "username"]
        );
        assert!(!document_view.is_empty());
        assert_eq!(document_view.len(), 5);
        assert_eq!(
            document_view.get("username").unwrap(),
            &DocumentViewValue::new(
                test_create_operation.operation_id(),
                &OperationValue::Text("bubu".to_owned()),
            )
        );
        assert_eq!(
            document_view.get("height").unwrap(),
            &DocumentViewValue::new(
                test_create_operation.operation_id(),
                &OperationValue::Float(3.5)
            ),
        );
        assert_eq!(
            document_view.get("age").unwrap(),
            &DocumentViewValue::new(
                test_create_operation.operation_id(),
                &OperationValue::Integer(28)
            ),
        );
        assert_eq!(
            document_view.get("is_admin").unwrap(),
            &DocumentViewValue::new(
                test_create_operation.operation_id(),
                &OperationValue::Boolean(false)
            ),
        );
        assert_eq!(
            document_view.get("profile_picture").unwrap(),
            &DocumentViewValue::new(
                test_create_operation.operation_id(),
                &OperationValue::Relation(expected_relation)
            ),
        );
        assert!(!is_edited);
        assert!(!is_deleted);
    }

    #[rstest]
    fn with_update_op(
        test_create_operation: OperationWithMeta,
        test_update_operation: OperationWithMeta,
        document_view_id: DocumentViewId,
        #[from(document_id)] relation_id: DocumentId,
    ) {
        let (view, is_edited, is_deleted) =
            reduce(&[test_create_operation.clone(), test_update_operation.clone()]);

        let document_view = DocumentView::new(&document_view_id, &view.unwrap());

        assert_eq!(
            document_view.get("username").unwrap(),
            &DocumentViewValue::new(
                test_create_operation.operation_id(),
                &OperationValue::Text("bubu".to_owned()),
            )
        );
        assert_eq!(
            document_view.get("height").unwrap(),
            &DocumentViewValue::new(
                test_create_operation.operation_id(),
                &OperationValue::Float(3.5)
            ),
        );
        assert_eq!(
            document_view.get("age").unwrap(),
            &DocumentViewValue::new(
                test_update_operation.operation_id(),
                &OperationValue::Integer(29)
            ),
        );
        assert_eq!(
            document_view.get("is_admin").unwrap(),
            &DocumentViewValue::new(
                test_update_operation.operation_id(),
                &OperationValue::Boolean(true)
            )
        );
        assert_eq!(
            document_view.get("profile_picture").unwrap(),
            &DocumentViewValue::new(
                test_create_operation.operation_id(),
                &OperationValue::Relation(Relation::new(relation_id))
            )
        );
        assert!(is_edited);
        assert!(!is_deleted);
    }
}
