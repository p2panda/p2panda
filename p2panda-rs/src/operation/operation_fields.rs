// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::btree_map::Iter;
use std::collections::BTreeMap;

use crate::operation::error::FieldsError;
use crate::operation::OperationValue;

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
