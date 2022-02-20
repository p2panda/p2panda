// SPDX-License-Identifier: AGPL-3.view-or-later

//! Types and methods for deriving and maintaining materialised documents.
use std::collections::btree_map::Iter as BTreeMapIter;
use std::collections::BTreeMap;

use crate::operation::OperationValue;

/// The materialised view of a `Document`. It's fields match the documents schema definition.
///
/// `DocumentViews` can be instantiated from a CREATE operation and then mutated with UPDATE
/// or DELETE operations.
#[derive(Debug, PartialEq, Clone)]
pub struct DocumentView(pub(crate) BTreeMap<String, OperationValue>);

impl DocumentView {
    /// Get a single value from this instance by it's key.
    pub fn get(&self, key: &str) -> Option<&OperationValue> {
        self.0.get(key)
    }

    /// Returns a vector containing the keys of this instance.
    pub fn keys(&self) -> Vec<String> {
        self.0.clone().into_keys().collect::<Vec<String>>()
    }

    /// Returns an iterator of existing instance fields.
    pub fn iter(&self) -> BTreeMapIter<String, OperationValue> {
        self.0.iter()
    }

    /// Returns the number of fields on this instance.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns true if the instance is empty, otherwise false.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}
// @TODO: This currently makes sure the wasm tests work as cddl does not have any wasm support
// (yet). Remove this with: https://github.com/p2panda/p2panda/issues/99
#[cfg(not(target_arch = "wasm32"))]
#[cfg(test)]
mod tests {
    use rstest::rstest;

    use crate::document::reduce;
    use crate::hash::Hash;
    use crate::operation::OperationValue;
    use crate::test_utils::fixtures::{
        create_operation, delete_operation, fields, hash, schema, update_operation,
    };

    #[rstest]
    fn gets_the_right_values(
        schema: Hash,
        #[from(hash)] prev_op_hash: Hash,
        #[from(hash)] relation: Hash,
    ) {
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

        // Reduce a single CREATE `Operation` into an `DocumentView`
        let (document_view, is_edited, is_deleted) = reduce(&[create_operation.clone()]);

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
        let (document_view, is_edited, is_deleted) =
            reduce(&[create_operation.clone(), update_operation.clone()]);

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
