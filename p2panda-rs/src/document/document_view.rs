// SPDX-License-Identifier: AGPL-3.0-or-later

//! Types and methods for deriving and maintaining materialised documents.
use std::collections::btree_map::Iter as BTreeMapIter;
use std::fmt::Display;

use crate::document::{DocumentViewFields, DocumentViewId, DocumentViewValue};
use crate::Human;

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
        write!(f, "{}", self.id)
    }
}

impl Human for DocumentView {
    fn display(&self) -> String {
        format!("<DocumentView {}>", self.id.display())
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use crate::document::traits::AsDocument;
    use crate::document::{DocumentBuilder, DocumentViewId, DocumentViewValue};
    use crate::identity::PublicKey;
    use crate::operation::traits::AsOperation;
    use crate::operation::{
        Operation, OperationAction, OperationBuilder, OperationFields, OperationId, OperationValue,
    };
    use crate::test_utils::fixtures::{
        create_operation, operation_fields, public_key, random_operation_id,
    };
    use crate::Human;

    #[rstest]
    fn from_single_create_op(
        create_operation: Operation,
        #[from(random_operation_id)] id: OperationId,
        public_key: PublicKey,
        operation_fields: OperationFields,
    ) {
        let (document, _) = DocumentBuilder::new(vec![(id.clone(), create_operation, public_key)])
            .build()
            .unwrap();
        let document_view = document.view().unwrap();

        assert!(!document_view.is_empty());
        assert_eq!(document_view.len(), 9);
        assert_eq!(document_view.keys(), operation_fields.keys());
        for key in operation_fields.keys() {
            assert_eq!(
                document_view.get(&key).unwrap(),
                &DocumentViewValue::new(&id, operation_fields.get(&key).unwrap(),),
            );
        }
    }

    #[rstest]
    fn with_update_op(create_operation: Operation, public_key: PublicKey) {
        let create_id = random_operation_id();
        let update_operation = OperationBuilder::new(&create_operation.schema_id())
            .action(OperationAction::Update)
            .fields(&[(
                "username",
                OperationValue::String("Panda Cafe!!!!".to_string()),
            )])
            .previous(&DocumentViewId::new(&[create_id.clone()]))
            .build()
            .unwrap();
        let update_id = random_operation_id();

        let operations = vec![
            (create_id, create_operation, public_key),
            (update_id.clone(), update_operation, public_key),
        ];

        let (document, _) = DocumentBuilder::new(operations).build().unwrap();
        let document_view = document.view().unwrap();

        assert_eq!(
            document_view.get("username").unwrap(),
            &DocumentViewValue::new(
                &update_id,
                &OperationValue::String("Panda Cafe!!!!".to_owned()),
            )
        );
    }

    #[rstest]
    fn string_representation(create_operation: Operation, public_key: PublicKey) {
        let id_1 = "0020b177ec1bf26dfb3b7010d473e6d44713b29b765b99c6e60ecbfae742de496543"
            .parse::<OperationId>()
            .unwrap();
        let id_2 = "0020d3235c8fe6f58608200851b83cd8482808eb81e4c6b4b17805bba57da9f16e79"
            .parse::<OperationId>()
            .unwrap();

        let (document, _) =
            DocumentBuilder::new(vec![(id_1.clone(), create_operation, public_key)])
                .build()
                .unwrap();

        // Fabricate a view id just for testing.
        let mut view = document.view().unwrap();
        let view_id = DocumentViewId::new(&[id_1.clone(), id_2.clone()]);
        view.id = view_id;

        assert_eq!(format!("{id_1}_{id_2}"), view.to_string());
        assert_eq!(view.display(), "<DocumentView 496543_f16e79>");
    }
}
