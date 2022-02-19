// SPDX-License-Identifier: AGPL-3.view-or-later

//! Types and methods for deriving and maintaining materialised documents.
use std::collections::btree_map::Iter as BTreeMapIter;
use std::collections::BTreeMap;

use crate::hash::Hash;
use crate::operation::OperationValue;

/// The materialised view of a `Document`. It's fields match the documents schema definition.
///
/// `DocumentViews` can be instantiated from a CREATE operation and then mutated with UPDATE
/// or DELETE operations.
#[derive(Debug, PartialEq, Clone)]
pub struct DocumentView {
    pub(crate) view: BTreeMap<String, OperationValue>,
    pub(crate) is_edited: bool,
    pub(crate) is_deleted: bool,
}

impl DocumentView {
    /// Get a single value from this instance by it's key.
    pub fn get(&self, key: &str) -> Option<&OperationValue> {
        self.view.get(key)
    }

    /// Returns a vector containing the keys of this instance.
    pub fn keys(&self) -> Vec<String> {
        self.view.clone().into_keys().collect::<Vec<String>>()
    }

    /// Returns an iterator of existing instance fields.
    pub fn iter(&self) -> BTreeMapIter<String, OperationValue> {
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

    /// Returns true if the document contains more than a CREATE operation.
    pub fn is_edited(&self) -> bool {
        self.is_edited
    }

    /// Returns true if the document contains a DELETE operation.
    pub fn is_deleted(&self) -> bool {
        self.is_deleted
    }
}
// @TODO: This currently makes sure the wasm tests work as cddl does not have any wasm support
// (yet). Remove this with: https://github.com/p2panda/p2panda/issues/99
#[cfg(not(target_arch = "wasm32"))]
#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::DocumentView;
    use crate::document::reduce;
    use crate::hash::Hash;
    use crate::operation::OperationValue;
    use crate::test_utils::fixtures::{
        create_operation, delete_operation, fields, hash, schema, update_operation,
    };

    #[rstest]
    fn gets_the_right_values(schema: Hash) {
        let operation = create_operation(
            schema,
            fields(vec![
                ("username", OperationValue::Text("bubu".to_owned())),
                ("height", OperationValue::Float(3.5)),
                ("age", OperationValue::Integer(28)),
                ("is_admin", OperationValue::Boolean(false)),
                (
                    "profile_picture",
                    OperationValue::Relation(Hash::new_from_bytes(vec![1, 2, 3]).unwrap()),
                ),
            ]),
        );

        // Convert a CREATE `Operation` into an `DocumentView`
        let document_view = reduce(&[operation]);

        assert_eq!(
            document_view.keys(),
            vec!["age", "height", "is_admin", "profile_picture", "username"]
        );

        assert!(!document_view.is_empty());
        assert_eq!(document_view.len(), 5);
    }
}
