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

    /// Return a shortened six character representation.
    pub fn short_repr(&self) -> String {
        format!("<DocumentView {}>", self.id.short_repr())
    }
}

impl Display for DocumentView {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.id)
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use crate::document::document_view_fields::DocumentViewValue;
    use crate::document::{reduce, DocumentId};
    use crate::operation::{
        AsVerifiedOperation, OperationId, OperationValue, Relation, VerifiedOperation,
    };
    use crate::test_utils::constants::HASH;
    use crate::test_utils::fixtures::{
        document_id, document_view_id, operation_fields, verified_operation,
    };

    use super::{DocumentView, DocumentViewId};

    #[rstest]
    fn from_single_create_op(
        verified_operation: VerifiedOperation,
        document_view_id: DocumentViewId,
    ) {
        let expected_relation = Relation::new(HASH.parse().unwrap());

        // Reduce a single CREATE `Operation`
        let (view, is_edited, is_deleted) = reduce(&[verified_operation.clone()]);

        let document_view = DocumentView::new(&document_view_id, &view.unwrap());

        assert_eq!(
            document_view.keys(),
            vec![
                "age",
                "height",
                "is_admin",
                "my_friends",
                "profile_picture",
                "username"
            ]
        );
        assert!(!document_view.is_empty());
        assert_eq!(document_view.len(), 6);
        assert_eq!(
            document_view.get("username").unwrap(),
            &DocumentViewValue::new(
                verified_operation.operation_id(),
                &OperationValue::Text("bubu".to_owned()),
            )
        );
        assert_eq!(
            document_view.get("height").unwrap(),
            &DocumentViewValue::new(
                verified_operation.operation_id(),
                &OperationValue::Float(3.5)
            ),
        );
        assert_eq!(
            document_view.get("age").unwrap(),
            &DocumentViewValue::new(
                verified_operation.operation_id(),
                &OperationValue::Integer(28)
            ),
        );
        assert_eq!(
            document_view.get("is_admin").unwrap(),
            &DocumentViewValue::new(
                verified_operation.operation_id(),
                &OperationValue::Boolean(false)
            ),
        );
        assert_eq!(
            document_view.get("profile_picture").unwrap(),
            &DocumentViewValue::new(
                verified_operation.operation_id(),
                &OperationValue::Relation(expected_relation)
            ),
        );
        assert!(!is_edited);
        assert!(!is_deleted);
    }

    #[rstest]
    fn with_update_op(
        #[from(verified_operation)] create_operation: VerifiedOperation,
        #[from(verified_operation)]
        #[with(Some(operation_fields(vec![
            ("username", OperationValue::Text("yahoo".to_owned())),
            ("height", OperationValue::Float(100.23)),
            ("age", OperationValue::Integer(12)),
            ("is_admin", OperationValue::Boolean(true)),
        ])), Some(HASH.parse().unwrap()))]
        update_operation: VerifiedOperation,
        document_view_id: DocumentViewId,
        #[from(document_id)] relation_id: DocumentId,
    ) {
        let (view, is_edited, is_deleted) =
            reduce(&[create_operation.clone(), update_operation.clone()]);

        let document_view = DocumentView::new(&document_view_id, &view.unwrap());

        assert_eq!(
            document_view.get("username").unwrap(),
            &DocumentViewValue::new(
                update_operation.operation_id(),
                &OperationValue::Text("yahoo".to_owned()),
            )
        );
        assert_eq!(
            document_view.get("height").unwrap(),
            &DocumentViewValue::new(
                update_operation.operation_id(),
                &OperationValue::Float(100.23)
            ),
        );
        assert_eq!(
            document_view.get("age").unwrap(),
            &DocumentViewValue::new(
                update_operation.operation_id(),
                &OperationValue::Integer(12)
            ),
        );
        assert_eq!(
            document_view.get("is_admin").unwrap(),
            &DocumentViewValue::new(
                update_operation.operation_id(),
                &OperationValue::Boolean(true)
            )
        );
        assert_eq!(
            document_view.get("profile_picture").unwrap(),
            &DocumentViewValue::new(
                create_operation.operation_id(),
                &OperationValue::Relation(Relation::new(relation_id))
            )
        );
        assert!(is_edited);
        assert!(!is_deleted);
    }

    #[rstest]
    fn string_representation(verified_operation: VerifiedOperation) {
        let operation_1 = "0020b177ec1bf26dfb3b7010d473e6d44713b29b765b99c6e60ecbfae742de496543"
            .parse::<OperationId>()
            .unwrap();
        let operation_2 = "0020d3235c8fe6f58608200851b83cd8482808eb81e4c6b4b17805bba57da9f16e79"
            .parse::<OperationId>()
            .unwrap();

        let document_view_id = DocumentViewId::new(&[operation_1, operation_2]).unwrap();
        let (view, _, _) = reduce(&[verified_operation]);
        let document_view = DocumentView::new(&document_view_id, &view.unwrap());

        assert_eq!(
            format!("{}", document_view),
            "0020b177ec1bf26dfb3b7010d473e6d44713b29b765b99c6e60ecbfae742de496543_0020d3235c8fe6f58608200851b83cd8482808eb81e4c6b4b17805bba57da9f16e79"
        );
        assert_eq!(document_view.short_repr(), "<DocumentView 496543_f16e79>");
    }
}
