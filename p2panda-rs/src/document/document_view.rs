// SPDX-License-Identifier: AGPL-3.0-or-later

//! Types and methods for deriving and maintaining materialised documents.
use std::borrow::BorrowMut;
use std::collections::btree_map::Iter as BTreeMapIter;
use std::collections::BTreeMap;
use std::convert::TryFrom;

use crate::document::error::DocumentViewError;
use crate::operation::{AsOperation, Operation, OperationValue, OperationWithMeta};

/// The materialised view of a reduced collection of `Operations` describing a document.
#[derive(Debug, PartialEq, Clone)]
pub enum DocumentView {
    /// The available document view contains fields and values for the document
    Available(BTreeMap<String, OperationValue>),

    /// A deleted document's view contains only the field names of the document and can not be
    /// updated further
    Deleted(Vec<String>),
}

impl DocumentView {
    /// Returns a new `DocumentView`.
    fn new() -> Self {
        Self::Available(BTreeMap::new())
    }

    /// Get a single value from this instance by it's key. Returns `None` when the document
    /// has been deleted.
    pub fn get(&self, key: &str) -> Option<&OperationValue> {
        match self {
            DocumentView::Available(view) => Some(view.get(key).unwrap()),
            DocumentView::Deleted(_) => None,
        }
    }

    /// Update this `DocumentView` from an UPDATE `Operation`.
    pub fn apply_update<T: AsOperation>(&mut self, operation: T) -> Result<(), DocumentViewError> {
        if operation.is_delete() {
            let new_self = self.as_deleted();
            return match new_self {
                Err(error) => Err(error),
                Ok(value) => {
                    *self = value;
                    Ok(())
                }
            }
        }

        let fields = operation.fields();
        match self {
            DocumentView::Available(view) => {
                for (key, value) in fields.unwrap().iter() {
                    view.insert(key.to_string(), value.to_owned());
                }
                Ok(())
            }
            DocumentView::Deleted(_) => Err(DocumentViewError::DocumentDeleted),
        }
    }

    /// Mark this document view deleted
    pub fn as_deleted(&self) -> Result<DocumentView, DocumentViewError> {
        match self {
            DocumentView::Available(_) => {
                Ok(DocumentView::Deleted(self.keys()))
            },
            DocumentView::Deleted(_) => {
                Err(DocumentViewError::DocumentDeleted)
            }
        }
    }

    /// Returns a vector containing the keys of this instance.
    pub fn keys(&self) -> Vec<String> {
        match self {
            DocumentView::Available(fields) => fields.clone().into_keys().collect::<Vec<String>>(),
            DocumentView::Deleted(fields) => fields.clone(),
        }
    }

    /// Returns an iterator of existing instance fields.
    pub fn iter(&self) -> Result<BTreeMapIter<String, OperationValue>, DocumentViewError> {
        match self {
            DocumentView::Available(fields) => Ok(fields.iter()),
            DocumentView::Deleted(_) => Err(DocumentViewError::DocumentDeleted),
        }
    }

    /// Returns the number of fields on this instance.
    pub fn len(&self) -> usize {
        match self {
            DocumentView::Available(fields) => fields.len(),
            DocumentView::Deleted(fields) => fields.len(),
        }
    }

    /// Returns true if the instance is empty, otherwise false.
    pub fn is_empty(&self) -> bool {
        match self {
            DocumentView::Available(fields) => fields.is_empty(),
            DocumentView::Deleted(fields) => fields.len() == 0,
        }
    }
}

impl Default for DocumentView {
    fn default() -> Self {
        DocumentView::new()
    }
}

impl TryFrom<Operation> for DocumentView {
    type Error = DocumentViewError;

    fn try_from(operation: Operation) -> Result<DocumentView, DocumentViewError> {
        if !operation.is_create() {
            return Err(DocumentViewError::NotCreateOperation);
        };

        let mut document_view: DocumentView = DocumentView::new();
        let fields = operation.fields();

        if let DocumentView::Available(ref mut view_inner) = document_view {
            if let Some(fields) = fields {
                for (key, value) in fields.iter() {
                    view_inner.insert(key.to_string(), value.to_owned());
                }
            }
        }

        Ok(document_view)
    }
}

impl TryFrom<OperationWithMeta> for DocumentView {
    type Error = DocumentViewError;

    fn try_from(operation: OperationWithMeta) -> Result<DocumentView, DocumentViewError> {
        if !operation.is_create() {
            return Err(DocumentViewError::NotCreateOperation);
        };

        let mut document_view: DocumentView = DocumentView::new();
        let fields = operation.fields();

        if let DocumentView::Available(ref mut view_inner) = document_view {
            if let Some(fields) = fields {
                for (key, value) in fields.iter() {
                    view_inner.insert(key.to_string(), value.to_owned());
                }
            }
        }

        Ok(document_view)
    }
}

impl From<BTreeMap<String, OperationValue>> for DocumentView {
    fn from(map: BTreeMap<String, OperationValue>) -> Self {
        Self::Available(map)
    }
}

// @TODO: This currently makes sure the wasm tests work as cddl does not have any wasm support
// (yet). Remove this with: https://github.com/p2panda/p2panda/issues/99
#[cfg(not(target_arch = "wasm32"))]
#[cfg(test)]
mod tests {
    use std::convert::{TryFrom, TryInto};

    use rstest::rstest;

    use crate::hash::Hash;
    use crate::operation::{AsOperation, Operation, OperationValue};
    use crate::schema::Schema;
    use crate::test_utils::fixtures::{
        create_operation, delete_operation, fields, hash, schema, update_operation,
    };

    use super::DocumentView;

    #[rstest]
    fn basic_methods(schema: Hash) {
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
        let doc_view: DocumentView = operation.try_into().unwrap();

        assert_eq!(
            doc_view.keys(),
            vec!["age", "height", "is_admin", "profile_picture", "username"]
        );

        assert!(!doc_view.is_empty());

        let empty_instance = DocumentView::new();
        assert!(empty_instance.is_empty());

        assert_eq!(doc_view.len(), 5)
    }

    #[rstest]
    fn try_from_operation(
        create_operation: Operation,
        update_operation: Operation,
        delete_operation: Operation,
    ) {
        // Convert a CREATE `Operation` into an `DocumentView`
        let doc_view: DocumentView = create_operation.clone().try_into().unwrap();

        let mut expected_view = DocumentView::new();
        if let DocumentView::Available(ref mut view) = expected_view {
            view.insert(
                "message".to_string(),
                create_operation
                    .fields()
                    .unwrap()
                    .get("message")
                    .unwrap()
                    .to_owned(),
            );
        }
        assert_eq!(doc_view, expected_view);

        // Convert an UPDATE or DELETE `Operation` into an `DocumentView`
        let instance_1 = DocumentView::try_from(update_operation);
        let instance_2 = DocumentView::try_from(delete_operation);

        assert!(instance_1.is_err());
        assert!(instance_2.is_err());
    }

    #[rstest]
    pub fn update(create_operation: Operation, update_operation: Operation) {
        let mut chat_view = DocumentView::try_from(create_operation.clone()).unwrap();
        chat_view.apply_update(update_operation.clone()).unwrap();

        let mut expected_chat_view = DocumentView::new();
        if let DocumentView::Available(ref mut view) = expected_chat_view {
            view.insert(
                "message".to_string(),
                create_operation
                    .fields()
                    .unwrap()
                    .get("message")
                    .unwrap()
                    .to_owned(),
            );

            view.insert(
                "message".to_string(),
                update_operation
                    .fields()
                    .unwrap()
                    .get("message")
                    .unwrap()
                    .to_owned(),
            );
        }

        assert_eq!(chat_view, expected_chat_view)
    }

    #[rstest]
    pub fn create_from_schema(#[from(hash)] schema_hash: Hash, create_operation: Operation) {
        // Instantiate "person" schema from CDDL string
        let chat_schema_definition = "
            chat = { (
                message: { type: \"str\", value: tstr }
            ) }
        ";

        let chat = Schema::new(&schema_hash, &chat_schema_definition.to_string()).unwrap();
        let chat_view = chat.instance_from_create(create_operation.clone()).unwrap();

        let mut expected_chat_view = DocumentView::new();
        if let DocumentView::Available(ref mut view) = expected_chat_view {
            view.insert(
                "message".to_string(),
                create_operation
                    .fields()
                    .unwrap()
                    .get("message")
                    .unwrap()
                    .to_owned(),
            );
        }

        assert_eq!(chat_view, expected_chat_view)
    }
}
