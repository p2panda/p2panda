// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::btree_map::Iter;
use std::collections::BTreeMap;
use std::convert::TryFrom;

use serde::{Deserialize, Serialize};

use crate::operation::{OperationError, OperationFieldsError, OperationValue};
use crate::Validate;

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

impl TryFrom<Vec<(&str, OperationValue)>> for OperationFields {
    type Error = OperationFieldsError;

    fn try_from(spec: Vec<(&str, OperationValue)>) -> Result<Self, Self::Error> {
        let mut operation_fields = OperationFields::new();
        for field in spec {
            operation_fields.add(field.0, field.1)?;
        }
        Ok(operation_fields)
    }
}

#[cfg(test)]
mod tests {
    use std::convert::TryFrom;

    use rstest::rstest;

    use crate::document::DocumentViewId;
    use crate::operation::{OperationId, PinnedRelationList};
    use crate::test_utils::fixtures::random_operation_id;

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
    fn pinned_relation_lists(
        #[from(random_operation_id)] operation_id_1: OperationId,
        #[from(random_operation_id)] operation_id_2: OperationId,
        #[from(random_operation_id)] operation_id_3: OperationId,
        #[from(random_operation_id)] operation_id_4: OperationId,
        #[from(random_operation_id)] operation_id_5: OperationId,
        #[from(random_operation_id)] operation_id_6: OperationId,
    ) {
        let document_view_id_1 = DocumentViewId::new(&[operation_id_1, operation_id_2]).unwrap();
        let document_view_id_2 = DocumentViewId::new(&[operation_id_3]).unwrap();
        let document_view_id_3 =
            DocumentViewId::new(&[operation_id_4, operation_id_5, operation_id_6]).unwrap();

        let relations = PinnedRelationList::new(vec![
            document_view_id_1,
            document_view_id_2,
            document_view_id_3,
        ]);

        let value = OperationValue::PinnedRelationList(relations);
        let mut fields = OperationFields::new();
        assert!(fields.add("locations", value).is_ok());
    }

    #[test]
    fn easy_operation_fields() {
        assert!(OperationFields::try_from(vec![("name", "boppety".into())]).is_ok());

        assert!(
            OperationFields::try_from(vec![("name", "boppety".into()), ("name", true.into())])
                .is_err()
        );
    }
}
