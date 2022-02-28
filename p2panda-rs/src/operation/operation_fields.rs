// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::btree_map::Iter;
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::hash::HashError;
use crate::operation::{OperationError, OperationFieldsError, Relation, RelationList};
use crate::Validate;

/// Enum of possible data types which can be added to the operations fields as values.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum OperationValue {
    /// Boolean value.
    #[serde(rename = "bool")]
    Boolean(bool),

    /// Signed integer value.
    #[serde(rename = "int")]
    Integer(i64),

    /// Floating point value.
    #[serde(rename = "float")]
    Float(f64),

    /// String value.
    #[serde(rename = "str")]
    Text(String),

    /// Reference to a document.
    #[serde(rename = "relation")]
    Relation(Relation),

    /// Reference to a list of documents.
    #[serde(rename = "relation_list")]
    RelationList(RelationList),
}

impl Validate for OperationValue {
    type Error = HashError;

    fn validate(&self) -> Result<(), Self::Error> {
        match self {
            Self::Relation(relation) => relation.validate(),
            Self::RelationList(relations) => relations.validate(),
            _ => Ok(()),
        }
    }
}

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
/// # Example
///
/// ```
/// # extern crate p2panda_rs;
/// # fn main() -> () {
/// # use p2panda_rs::operation::{OperationFields, OperationValue, AsOperation};
/// let mut fields = OperationFields::new();
/// fields
///     .add("title", OperationValue::Text("Hello, Panda!".to_owned()))
///     .unwrap();
/// }
/// ```
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
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
    pub fn add(&mut self, name: &str, value: OperationValue) -> Result<(), OperationFieldsError> {
        if self.0.contains_key(name) {
            return Err(OperationFieldsError::FieldDuplicate);
        }

        self.0.insert(name.to_owned(), value);

        Ok(())
    }

    /// Overwrites an already existing field with a new value.
    pub fn update(
        &mut self,
        name: &str,
        value: OperationValue,
    ) -> Result<(), OperationFieldsError> {
        if !self.0.contains_key(name) {
            return Err(OperationFieldsError::UnknownField);
        }

        self.0.insert(name.to_owned(), value);

        Ok(())
    }

    /// Removes an existing field from this instance.
    pub fn remove(&mut self, name: &str) -> Result<(), OperationFieldsError> {
        if !self.0.contains_key(name) {
            return Err(OperationFieldsError::UnknownField);
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

impl Validate for OperationFields {
    type Error = OperationError;

    fn validate(&self) -> Result<(), Self::Error> {
        for (_, value) in self.iter() {
            value.validate()?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use crate::document::DocumentId;
    use crate::operation::{Relation, RelationList};
    use crate::test_utils::fixtures::random_document_id;

    use super::{OperationFields, OperationValue};

    #[test]
    fn operation_fields() {
        let mut fields = OperationFields::new();

        // Detect duplicate
        fields
            .add("message", OperationValue::Text("Hello, Panda!".to_owned()))
            .unwrap();

        // Have to use `update` to change fields
        assert!(fields
            .add("message", OperationValue::Text("Huhu".to_owned()))
            .is_err());

        assert!(fields
            .update("message", OperationValue::Text("Huhu".to_owned()))
            .is_ok());

        // Bail when key does not exist
        assert!(fields
            .update("imagine", OperationValue::Text("Pandaparty".to_owned()))
            .is_err());

        assert_eq!(fields.keys(), vec!["message"]);

        assert!(fields.remove("message").is_ok());

        assert_eq!(fields.len(), 0);
    }

    #[rstest]
    fn relation_lists(
        #[from(random_document_id)] document_1: DocumentId,
        #[from(random_document_id)] document_2: DocumentId,
    ) {
        let relations =
            RelationList::new(vec![Relation::new(document_1), Relation::new(document_2)]);

        let mut fields = OperationFields::new();
        assert!(fields
            .add("locations", OperationValue::RelationList(relations))
            .is_ok());
    }
}
