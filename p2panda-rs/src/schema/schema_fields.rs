// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::btree_map::Iter;
use std::collections::BTreeMap;

use crate::schema::error::SchemaFieldError;
use crate::schema::validate::validate_field_name;
use crate::schema::FieldType;
use crate::Validate;

/// The fields definitions of a [`Schema`].
#[derive(Clone, Debug, PartialEq, Default, Eq)]
pub struct SchemaFields(BTreeMap<String, FieldType>);

impl SchemaFields {
    /// Creates a new schema fields instance from a vector of key values.
    pub fn new(fields: &[(&str, FieldType)]) -> Result<Self, SchemaFieldError> {
        // Check for duplicate field keys in the passed array.
        let mut keys: Vec<&str> = fields.iter().map(|(key, _)| *key).collect();
        keys.dedup();
        if keys.len() != fields.len() {
            return Err(SchemaFieldError::DuplicateFields);
        }

        // Construct schema fields map.
        let mut schema_fields = BTreeMap::new();
        for (key, value) in fields {
            schema_fields.insert(key.to_string(), value.to_owned());
        }
        let schema_fields = Self(schema_fields);

        // Validate the schema fields.
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
}

impl Validate for SchemaFields {
    type Error = SchemaFieldError;

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
    fn validate(&self) -> Result<(), Self::Error> {
        // Validate schema field names.
        for name in self.keys() {
            if !validate_field_name(&name) {
                return Err(SchemaFieldError::MalformedSchemaFieldName);
            }
        }

        // Check there are no more than 1024 fields.
        if self.0.len() > 1024 {
            return Err(SchemaFieldError::TooManyFields);
        }

        // Verify there is at least one field.
        if self.is_empty() {
            return Err(SchemaFieldError::ZeroFields);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use crate::schema::FieldType;

    use super::SchemaFields;

    #[rstest]
    #[case(vec![("message", FieldType::String)])]
    #[should_panic(expected = "Schema fields cannot contain duplicate field names")]
    #[case(vec![("message", FieldType::String), ("message", FieldType::String)])]
    #[should_panic(expected = "Schema fields must contain at least one entry")]
    #[case(vec![])]
    #[should_panic(expected = "Schema field found with invalid name")]
    #[case(vec![("123", FieldType::String)])]
    #[should_panic(expected = "Schema field found with invalid name")]
    #[case(vec![("$$$", FieldType::String)])]
    #[should_panic(expected = "Schema field found with invalid name")]
    #[case(vec![("why      spaces", FieldType::String)])]
    #[should_panic(expected = "Schema field found with invalid name")]
    #[case(vec![("a_really_really_really_really_really_really_really_really_really_really_really_really_really_really_really_long_name", FieldType::String)])]
    fn validates_fields(#[case] fields: Vec<(&str, FieldType)>) {
        SchemaFields::new(&fields)
            .map_err(|err| err.to_string())
            .unwrap();
    }

    #[test]
    fn validates_when_too_many_fields() {
        let keys: Vec<String> = (0..2000_u32).map(|key| format!("field_{key}")).collect();
        let fields: Vec<(&str, FieldType)> = keys
            .iter()
            .map(|key| (key.as_str(), FieldType::String))
            .collect();

        let result = SchemaFields::new(&fields);

        assert_eq!(
            &result.unwrap_err().to_string(),
            "Schema fields contains more than 1024 fields"
        );
    }
}
