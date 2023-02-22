// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::btree_map::Iter;
use std::collections::BTreeMap;

use super::error::SchemaFieldError;
use super::validate::validate_field_name;
use super::FieldType;

/// The fields of a schema.
#[derive(Clone, Debug, PartialEq, Default, Eq)]
pub struct SchemaFields(BTreeMap<String, FieldType>);

impl SchemaFields {
    /// Creates a new schema fields instance to add data to.
    pub fn new(fields: &[(&str, FieldType)]) -> Result<Self, SchemaFieldError> {
        // Check for duplicate field keys in the passed array.
        let mut keys: Vec<&str> = fields.iter().map(|(key, _)| *key).collect();
        keys.dedup();

        if !(keys.len() == fields.len()) {
            return Err(SchemaFieldError::DuplicateFields);
        }

        let mut schema_fields = BTreeMap::new();
        for (key, value) in fields {
            schema_fields.insert(key.to_string(), value.to_owned());
        }

        let schema_fields = Self(schema_fields);
        schema_fields.validate()?;
        Ok(schema_fields)
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
    pub fn get(&self, name: &str) -> Option<&FieldType> {
        if !self.0.contains_key(name) {
            return None;
        }

        self.0.get(name)
    }

    /// Returns an array of existing schema field keys.
    pub fn keys(&self) -> Vec<String> {
        self.0.keys().cloned().collect()
    }

    /// Returns an iterator of existing schema fields.
    pub fn iter(&self) -> Iter<String, FieldType> {
        self.0.iter()
    }

    /// Performs the following validation steps:
    ///
    /// Field name:
    ///   1. It must be at most 64 characters long
    ///   2. It begins with a letter
    ///   3. It uses only alphanumeric characters, digits and the underscore character
    /// Fields:
    ///   1. At least one field
    ///   2. No more than 1024 fields
    ///
    /// Note: The underlying datatype BTreeMap cannot contain duplicate fields and orders fields
    /// by their key. This already fulfils two requirements for SchemaFields.
    pub fn validate(&self) -> Result<(), SchemaFieldError> {
        for name in self.keys() {
            if !validate_field_name(&name) {
                return Err(SchemaFieldError::MalformedSchemaFieldName);
            }
        }

        if !self.0.len() < 1024 {
            return Err(SchemaFieldError::MaxSchemaFieldsReached);
        }

        if self.0.len() == 0 {
            return Err(SchemaFieldError::ZeroFields);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {}
