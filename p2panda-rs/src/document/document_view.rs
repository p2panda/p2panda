// SPDX-License-Identifier: AGPL-3.0-or-later

//! Types and methods for deriving and maintaining materialised documents.
use std::collections::btree_map::Iter as BTreeMapIter;
use std::collections::BTreeMap;

use crate::hash::Hash;
use crate::operation::OperationValue;

/// The ID of a document view. Contains the hash id of the document, and the hash ids of the
/// current document graph tips.
#[derive(Debug, PartialEq, Clone)]
pub struct DocumentViewId {
    document_id: Hash,
    view_id: Vec<Hash>,
}

impl DocumentViewId {
    /// Create a new document view id.
    pub fn new(document_id: Hash, view_id: Vec<Hash>) -> Self {
        Self {
            document_id,
            view_id,
        }
    }

    /// Get just the document id.
    pub fn document_id(&self) -> &Hash {
        &self.document_id
    }

    /// Get just the view id.
    pub fn view_id(&self) -> &[Hash] {
        self.view_id.as_slice()
    }
}

type FieldKey = String;

/// The materialised view of a `Document`. It's fields match the documents schema definition.
///
/// `DocumentViews` can be instantiated from a CREATE operation and then mutated with UPDATE
/// or DELETE operations.
#[derive(Debug, PartialEq, Clone)]
pub struct DocumentView {
    pub(crate) id: DocumentViewId,
    pub(crate) view: BTreeMap<FieldKey, OperationValue>,
}

impl DocumentView {
    /// Construct a document view.
    ///
    /// Requires the DocumentViewId and field values to be calculated seperately and then passed in
    /// during construction.
    pub fn new(id: DocumentViewId, view: BTreeMap<FieldKey, OperationValue>) -> Self {
        Self { id, view }
    }

    /// Get the id of this document view.
    pub fn id(&self) -> &[Hash] {
        self.id.view_id()
    }

    /// Get the id of this document.
    pub fn document_id(&self) -> &Hash {
        self.id.document_id()
    }

    /// Get the document view id.
    pub fn document_view_id(&self) -> &DocumentViewId {
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
    use rstest::rstest;

    use crate::document::reduce;
    use crate::hash::Hash;
    use crate::operation::{OperationValue, Relation};
    use crate::schema::SchemaId;
    use crate::test_utils::fixtures::{
        create_operation, delete_operation, fields, random_hash, schema, update_operation,
    };

    use super::{DocumentView, DocumentViewId};

    #[rstest]
    fn gets_the_right_values(
        schema: SchemaId,
        #[from(random_hash)] prev_op_hash: Hash,
        #[from(random_hash)] document_id: Hash,
        #[from(random_hash)] relation: Hash,
        #[from(random_hash)] view_id: Hash,
    ) {
        let document_view_id = DocumentViewId {
            document_id,
            view_id: vec![view_id],
        };

        let relation = Relation::new(relation);

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

        let document_view = DocumentView::new(document_view_id.clone(), view);

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

        let delete_operation = delete_operation(schema, vec![prev_op_hash]);

        // Reduce again now with a DELETE operation as well
        let (_document_view, is_edited, is_deleted) =
            reduce(&[create_operation, update_operation, delete_operation]);

        assert!(is_edited);
        assert!(is_deleted);
    }
}
