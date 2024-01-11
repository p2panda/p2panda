// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::btree_map::Iter;
use std::collections::BTreeMap;

use crate::operation::traits::{Fielded, Identifiable};
use crate::operation::{OperationFields, OperationId, OperationValue};

/// The current value of a document fiew field as well as the id of the operation it came from.
#[derive(Clone, Debug, PartialEq)]
pub struct DocumentViewValue {
    operation_id: OperationId,
    value: OperationValue,
}

impl DocumentViewValue {
    /// Returns a `DocumentViewValue` constructed from an `OperationId` and `OperationValue`.
    pub fn new(operation_id: &OperationId, value: &OperationValue) -> Self {
        Self {
            operation_id: operation_id.clone(),
            value: value.clone(),
        }
    }

    /// Get the OperationId of this document value.
    pub fn id(&self) -> &OperationId {
        &self.operation_id
    }

    /// Get the OperationValue of this document value.
    pub fn value(&self) -> &OperationValue {
        &self.value
    }
}

/// A key value map of field keys to `DocumentViewValues`.
#[derive(Clone, Debug, PartialEq)]
pub struct DocumentViewFields(BTreeMap<String, DocumentViewValue>);

impl DocumentViewFields {
    /// Creates a new fields instance to add data to.
    pub fn new() -> Self {
        Self(BTreeMap::new())
    }

    /// Creates a new populated fields instance from existing OperationFields and OperationId.
    pub fn new_from_operation_fields(id: &OperationId, fields: &OperationFields) -> Self {
        let mut document_view_fields = DocumentViewFields::new();

        for (name, value) in fields.iter() {
            document_view_fields.insert(name, DocumentViewValue::new(id, value));
        }

        document_view_fields
    }

    /// Insert a new field to this instance.
    pub fn insert(&mut self, name: &str, value: DocumentViewValue) -> Option<DocumentViewValue> {
        self.0.insert(name.to_owned(), value)
    }

    /// Returns the number of added fields.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns true when no field is given.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Returns a field value.
    pub fn get(&self, name: &str) -> Option<&DocumentViewValue> {
        if !self.0.contains_key(name) {
            return None;
        }

        self.0.get(name)
    }

    /// Returns an array of existing document view keys.
    pub fn keys(&self) -> Vec<String> {
        self.0.keys().cloned().collect()
    }

    /// Returns an iterator of existing document view fields.
    pub fn iter(&self) -> Iter<String, DocumentViewValue> {
        self.0.iter()
    }
}

impl Default for DocumentViewFields {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Fielded + Identifiable> From<T> for DocumentViewFields {
    fn from(operation: T) -> Self {
        let mut document_view_fields = DocumentViewFields::new();

        if let Some(fields) = operation.fields() {
            for (name, value) in fields.iter() {
                document_view_fields.insert(name, DocumentViewValue::new(&operation.id(), value));
            }
        }

        document_view_fields
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use crate::document::{DocumentViewFields, DocumentViewValue};
    use crate::identity::KeyPair;
    use crate::operation::traits::{Fielded, Identifiable};
    use crate::operation::{OperationBuilder, OperationId, OperationValue};
    use crate::schema::SchemaId;
    use crate::test_utils::constants::TIMESTAMP;
    use crate::test_utils::fixtures::random_operation_id;
    use crate::test_utils::fixtures::{key_pair, schema_id};

    #[rstest]
    fn construct_fields(#[from(random_operation_id)] value_id: OperationId) {
        let mut fields = DocumentViewFields::new();

        fields.insert(
            "name",
            DocumentViewValue::new(&value_id, &OperationValue::String("ʕ •ᴥ•ʔ Cafe!".into())),
        );
        fields.insert(
            "owner",
            DocumentViewValue::new(&value_id, &OperationValue::String("しろくま".into())),
        );
        fields.insert(
            "house-number",
            DocumentViewValue::new(&value_id, &OperationValue::Integer(12)),
        );

        assert_eq!(fields.len(), 3);
        assert!(!fields.is_empty());
        assert_eq!(
            fields.get("name").unwrap(),
            &DocumentViewValue::new(&value_id, &OperationValue::String("ʕ •ᴥ•ʔ Cafe!".into()))
        );
        assert_eq!(
            fields.get("owner").unwrap(),
            &DocumentViewValue::new(&value_id, &OperationValue::String("しろくま".into()))
        );
        assert_eq!(
            fields.get("house-number").unwrap(),
            &DocumentViewValue::new(&value_id, &OperationValue::Integer(12))
        );
    }

    #[rstest]
    fn from_published_operation(key_pair: KeyPair, schema_id: SchemaId) {
        let operation = OperationBuilder::new(&schema_id, TIMESTAMP)
            .fields(&[("year", 2020.into())])
            .sign(&key_pair)
            .unwrap();

        let document_view_fields = DocumentViewFields::from(operation.clone());
        let operation_fields = operation.fields().unwrap();
        assert_eq!(document_view_fields.len(), operation_fields.len());
    }

    #[rstest]
    fn new_from_operation_fields(key_pair: KeyPair, schema_id: SchemaId) {
        let operation = OperationBuilder::new(&schema_id, TIMESTAMP)
            .fields(&[("year", 2020.into())])
            .sign(&key_pair)
            .unwrap();

        let document_view_fields = DocumentViewFields::new_from_operation_fields(
            operation.id(),
            &operation.fields().unwrap(),
        );
        let operation_fields = operation.fields().unwrap();
        assert_eq!(document_view_fields.len(), operation_fields.len());
    }
}
