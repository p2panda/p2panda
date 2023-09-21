// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::btree_map::Iter;
use std::collections::BTreeMap;

use crate::operation_v2::operation::error::FieldsError;
use crate::operation_v2::operation::OperationValue;

/// Operation fields are used to store application data. They are implemented as a simple key/value
/// store with support for a limited number of data types (see [`OperationValue`] for further
/// documentation on this). A `OperationFields` instance can contain any number and types of
/// fields. However, when a `OperationFields` instance is attached to a `Operation`, the
/// operation's schema determines which fields may be used.
///
/// Internally operation fields use sorted B-Tree maps to assure ordering of the fields. If the
/// operation fields would not be sorted consistently we would get different hash results for the
/// same contents.
///
/// ## Example
///
/// ```
/// # extern crate p2panda_rs;
/// # fn main() -> () {
/// # use p2panda_rs::operation::OperationFields;
/// let mut fields = OperationFields::new();
/// fields
///     .insert("title", "Hello, Panda!".into())
///     .unwrap();
/// }
/// ```
#[derive(Clone, Debug, PartialEq, Default)]
pub struct OperationFields(BTreeMap<String, OperationValue>);

impl OperationFields {
    /// Creates a new fields instance to add data to.
    pub fn new() -> Self {
        Self(BTreeMap::new())
    }

    /// Returns the number of added fields.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns true when no field is given.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Adds a new field to this instance.
    ///
    /// A field is a simple key/value pair.
    pub fn insert(&mut self, name: &str, value: OperationValue) -> Result<(), FieldsError> {
        if self.0.contains_key(name) {
            return Err(FieldsError::FieldDuplicate(name.to_owned()));
        }

        self.0.insert(name.to_owned(), value);

        Ok(())
    }

    /// Overwrites an already existing field with a new value.
    pub fn update(&mut self, name: &str, value: OperationValue) -> Result<(), FieldsError> {
        if !self.0.contains_key(name) {
            return Err(FieldsError::UnknownField);
        }

        self.0.insert(name.to_owned(), value);

        Ok(())
    }

    /// Removes an existing field from this instance.
    pub fn remove(&mut self, name: &str) -> Result<(), FieldsError> {
        if !self.0.contains_key(name) {
            return Err(FieldsError::UnknownField);
        }

        self.0.remove(name);

        Ok(())
    }

    /// Returns a field value.
    pub fn get(&self, name: &str) -> Option<&OperationValue> {
        if !self.0.contains_key(name) {
            return None;
        }

        self.0.get(name)
    }

    /// Returns an array of existing operation keys.
    pub fn keys(&self) -> Vec<String> {
        self.0.keys().cloned().collect()
    }

    /// Returns an iterator of existing operation fields.
    pub fn iter(&self) -> Iter<String, OperationValue> {
        self.0.iter()
    }
}

impl From<Vec<(&str, OperationValue)>> for OperationFields {
    fn from(spec: Vec<(&str, OperationValue)>) -> Self {
        let mut operation_fields = OperationFields::new();

        for field in spec {
            if operation_fields.insert(field.0, field.1).is_err() {
                // Silently ignore duplicates errors .. the underlying data type takes care of that
                // for us!
            }
        }

        operation_fields
    }
}

/*#[cfg(test)]
mod tests {
    use rstest::rstest;

    use crate::document::DocumentViewId;
    use crate::operation_v2::body::{OperationId, PinnedRelationList};
    use crate::test_utils::fixtures::random_operation_id;

    use super::{OperationFields, OperationValue};

    #[test]
    fn operation_fields() {
        let mut fields = OperationFields::new();

        // Detect duplicate
        fields
            .insert(
                "message",
                OperationValue::String("Hello, Panda!".to_owned()),
            )
            .unwrap();

        // Have to use `update` to change fields
        assert!(fields
            .insert("message", OperationValue::String("Huhu".to_owned()))
            .is_err());

        assert!(fields
            .update("message", OperationValue::String("Huhu".to_owned()))
            .is_ok());

        // Bail when key does not exist
        assert!(fields
            .update("imagine", OperationValue::String("Pandaparty".to_owned()))
            .is_err());

        assert_eq!(fields.keys(), vec!["message"]);

        assert!(fields.remove("message").is_ok());

        assert_eq!(fields.len(), 0);
    }

    #[rstest]
    fn pinned_relation_lists(
        #[from(random_operation_id)] operation_id_1: OperationId,
        #[from(random_operation_id)] operation_id_2: OperationId,
        #[from(random_operation_id)] operation_id_3: OperationId,
        #[from(random_operation_id)] operation_id_4: OperationId,
        #[from(random_operation_id)] operation_id_5: OperationId,
        #[from(random_operation_id)] operation_id_6: OperationId,
    ) {
        let document_view_id_1 = DocumentViewId::new(&[operation_id_1, operation_id_2]);
        let document_view_id_2 = DocumentViewId::new(&[operation_id_3]);
        let document_view_id_3 =
            DocumentViewId::new(&[operation_id_4, operation_id_5, operation_id_6]);

        let relations = PinnedRelationList::new(vec![
            document_view_id_1,
            document_view_id_2,
            document_view_id_3,
        ]);

        let value = OperationValue::PinnedRelationList(relations);
        let mut fields = OperationFields::new();
        assert!(fields.insert("locations", value).is_ok());
    }

    #[test]
    fn from_vec() {
        let fields = OperationFields::from(vec![
            ("message", "Hello, Panda!".into()),
            ("message", "Duplicates are ignored".into()),
            ("is_cute", true.into()),
        ]);

        assert_eq!(
            fields.get("message").unwrap(),
            &OperationValue::String("Hello, Panda!".into())
        );
        assert_eq!(
            fields.get("is_cute").unwrap(),
            &OperationValue::Boolean(true)
        );
    }
}*/
