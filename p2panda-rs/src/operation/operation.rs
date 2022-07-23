// SPDX-License-Identifier: AGPL-3.0-or-later

use std::hash::{Hash as StdHash, Hasher};

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_repr::{Deserialize_repr, Serialize_repr};

use crate::document::DocumentViewId;
use crate::operation::{AsOperation, OperationEncoded, OperationError, OperationFields};
use crate::schema::SchemaId;
use crate::Validate;

/// Operation format versions to introduce API changes in the future.
///
/// Operations contain the actual data of applications in the p2panda network and will be stored
/// for an indefinite time on different machines. To allow an upgrade path in the future and
/// support backwards compatibility for old data we can use this version number.
#[derive(Clone, Debug, PartialEq, Eq, Serialize_repr, Deserialize_repr)]
#[serde(untagged)]
#[repr(u64)]
pub enum OperationVersion {
    /// The default version number.
    Default = 1,
}

impl Copy for OperationVersion {}

/// Operations are categorised by their action type.
///
/// An action defines the operation format and if this operation creates, updates or deletes a data
/// document.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OperationAction {
    /// Operation creates a new document.
    Create,

    /// Operation updates an existing document.
    Update,

    /// Operation deletes an existing document.
    Delete,
}

impl OperationAction {
    /// Returns the operation action as a string.
    pub fn as_str(&self) -> &str {
        match self {
            OperationAction::Create => "create",
            OperationAction::Update => "update",
            OperationAction::Delete => "delete",
        }
    }

    /// Returns the operation action encoded as u64.
    pub fn as_u64(&self) -> u64 {
        match self {
            OperationAction::Create => 0,
            OperationAction::Update => 1,
            OperationAction::Delete => 2,
        }
    }
}

impl Serialize for OperationAction {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u64(self.as_u64())
    }
}

impl<'de> Deserialize<'de> for OperationAction {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let action = u64::deserialize(deserializer)?;

        match action {
            0 => Ok(OperationAction::Create),
            1 => Ok(OperationAction::Update),
            2 => Ok(OperationAction::Delete),
            _ => Err(serde::de::Error::custom("Unknown operation action")),
        }
    }
}

/// Operations describe data mutations of "documents" in the p2panda network. Authors send
/// operations to CREATE, UPDATE or DELETE documents.
///
/// The data itself lives in the "fields" object and is formed after an operation schema.
///
/// Starting from an initial CREATE operation, the following collection of UPDATE operations build
/// up a causal graph of mutations which can be resolved into a single object during a
/// "materialisation" process. If a DELETE operation is published it signals the deletion of the
/// entire graph and no more UPDATE operations should be published.
///
/// All UPDATE and DELETE operations have a `previous_operations` field which contains a vector of
/// operation ids which identify the known branch tips at the time of publication. These allow
/// us to build the graph and retain knowledge of the graph state at the time the specific
/// operation was published.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Operation {
    /// Describes if this operation creates, updates or deletes data.
    action: OperationAction,

    /// Hash of schema describing format of operation fields.
    schema: SchemaId,

    /// Version schema of this operation.
    version: OperationVersion,

    /// Optional DocumentViewId containing the operation ids directly preceding this one
    /// in the document.
    #[serde(skip_serializing_if = "Option::is_none")]
    previous_operations: Option<DocumentViewId>,

    /// Optional fields map holding the operation data.
    #[serde(skip_serializing_if = "Option::is_none")]
    fields: Option<OperationFields>,
}

impl Operation {
    /// Returns new CREATE operation.
    ///
    /// ## Example
    ///
    /// ```
    /// # extern crate p2panda_rs;
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// use p2panda_rs::hash::Hash;
    /// use p2panda_rs::operation::{AsOperation, Operation, OperationFields, OperationValue};
    /// use p2panda_rs::schema::SchemaId;
    ///
    /// let msg_schema = SchemaId::new("zoo_0020c65567ae37efea293e34a9c7d13f8f2bf23dbdc3b5c7b9ab46293111c48fc78b")?;
    /// let mut msg_fields = OperationFields::new();
    ///
    /// msg_fields
    ///     .add(
    ///         "Zoo",
    ///         OperationValue::Text("Pandas, Doggos, Cats, and Parrots!".to_owned()),
    ///     )
    ///     .unwrap();
    ///
    /// let create_operation = Operation::new_create(msg_schema, msg_fields)?;
    ///
    /// assert_eq!(AsOperation::is_create(&create_operation), true);
    ///
    /// # Ok(())
    /// # }
    /// ```
    pub fn new_create(schema: SchemaId, fields: OperationFields) -> Result<Self, OperationError> {
        let operation = Self {
            action: OperationAction::Create,
            version: OperationVersion::Default,
            schema,
            previous_operations: None,
            fields: Some(fields),
        };

        operation.validate()?;

        Ok(operation)
    }

    /// Returns new UPDATE operation.
    pub fn new_update(
        schema: SchemaId,
        previous_operations: DocumentViewId,
        fields: OperationFields,
    ) -> Result<Self, OperationError> {
        let operation = Self {
            action: OperationAction::Update,
            version: OperationVersion::Default,
            schema,
            previous_operations: Some(previous_operations),
            fields: Some(fields),
        };

        operation.validate()?;

        Ok(operation)
    }

    /// Returns new DELETE operation.
    pub fn new_delete(
        schema: SchemaId,
        previous_operations: DocumentViewId,
    ) -> Result<Self, OperationError> {
        let operation = Self {
            action: OperationAction::Delete,
            version: OperationVersion::Default,
            schema,
            previous_operations: Some(previous_operations),
            fields: None,
        };

        operation.validate()?;

        Ok(operation)
    }

    /// Encodes operation in CBOR format and returns bytes.
    pub fn to_cbor(&self) -> Vec<u8> {
        let mut cbor_bytes = Vec::new();
        ciborium::ser::into_writer(&self, &mut cbor_bytes).unwrap();
        cbor_bytes
    }
}

impl AsOperation for Operation {
    /// Returns action type of operation.
    fn action(&self) -> OperationAction {
        self.action.to_owned()
    }

    /// Returns version of operation.
    fn version(&self) -> OperationVersion {
        self.version.to_owned()
    }

    /// Returns schema of operation.
    fn schema(&self) -> SchemaId {
        self.schema.to_owned()
    }

    /// Returns application data fields of operation.
    fn fields(&self) -> Option<OperationFields> {
        self.fields.clone()
    }

    /// Returns known previous operations vector of this operation.
    fn previous_operations(&self) -> Option<DocumentViewId> {
        self.previous_operations.clone()
    }
}

/// Decodes an encoded operation and returns it.
impl From<&OperationEncoded> for Operation {
    fn from(operation_encoded: &OperationEncoded) -> Self {
        ciborium::de::from_reader(&operation_encoded.to_bytes()[..]).unwrap()
    }
}

impl PartialEq for Operation {
    fn eq(&self, other: &Self) -> bool {
        self.to_cbor() == other.to_cbor()
    }
}

impl Eq for Operation {}

impl StdHash for Operation {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.to_cbor().hash(state);
    }
}

impl Validate for Operation {
    type Error = OperationError;

    fn validate(&self) -> Result<(), Self::Error> {
        // CREATE and UPDATE operations can not have empty fields.
        if !self.is_delete() && (!self.has_fields() || self.fields().unwrap().is_empty()) {
            return Err(OperationError::EmptyFields);
        }

        // DELETE must have empty fields
        if self.is_delete() && self.has_fields() {
            return Err(OperationError::DeleteWithFields);
        }

        // UPDATE and DELETE operations must contain previous_operations.
        if !self.is_create() && (!self.has_previous_operations()) {
            return Err(OperationError::EmptyPreviousOperations);
        }

        // CREATE operations must not contain previous_operations.
        if self.is_create() && (self.has_previous_operations()) {
            return Err(OperationError::ExistingPreviousOperations);
        }

        // Validate fields
        if self.has_fields() {
            self.fields().unwrap().validate()?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::convert::TryFrom;

    use rstest::rstest;
    use rstest_reuse::apply;

    use crate::document::{DocumentId, DocumentViewId};
    use crate::operation::{AsOperation, OperationEncoded, OperationValue, Relation};
    use crate::schema::SchemaId;
    use crate::test_utils::fixtures::{
        operation_fields, random_document_id, random_document_view_id, schema_id,
    };
    use crate::test_utils::templates::many_valid_operations;
    use crate::Validate;

    use super::{Operation, OperationAction, OperationFields, OperationVersion};

    #[test]
    fn stringify_action() {
        assert_eq!(OperationAction::Create.as_str(), "create");
        assert_eq!(OperationAction::Update.as_str(), "update");
        assert_eq!(OperationAction::Delete.as_str(), "delete");
    }

    #[rstest]
    fn operation_validation(
        operation_fields: OperationFields,
        schema_id: SchemaId,
        #[from(random_document_view_id)] prev_op_id: DocumentViewId,
    ) {
        let invalid_create_operation_1 = Operation {
            action: OperationAction::Create,
            version: OperationVersion::Default,
            schema: schema_id.clone(),
            previous_operations: None,
            // CREATE operations must contain fields
            fields: None, // Error
        };

        assert!(invalid_create_operation_1.validate().is_err());

        let invalid_create_operation_2 = Operation {
            action: OperationAction::Create,
            version: OperationVersion::Default,
            schema: schema_id.clone(),
            // CREATE operations must not contain previous_operations
            previous_operations: Some(prev_op_id.clone()), // Error
            fields: Some(operation_fields.clone()),
        };

        assert!(invalid_create_operation_2.validate().is_err());

        let invalid_update_operation_1 = Operation {
            action: OperationAction::Update,
            version: OperationVersion::Default,
            schema: schema_id.clone(),
            // UPDATE operations must contain previous_operations
            previous_operations: None, // Error
            fields: Some(operation_fields.clone()),
        };

        assert!(invalid_update_operation_1.validate().is_err());

        let invalid_update_operation_2 = Operation {
            action: OperationAction::Update,
            version: OperationVersion::Default,
            schema: schema_id.clone(),
            previous_operations: Some(prev_op_id.clone()),
            // UPDATE operations must contain fields
            fields: None, // Error
        };

        assert!(invalid_update_operation_2.validate().is_err());

        let invalid_delete_operation_1 = Operation {
            action: OperationAction::Delete,
            version: OperationVersion::Default,
            schema: schema_id.clone(),
            // DELETE operations must contain previous_operations
            previous_operations: None, // Error
            fields: None,
        };

        assert!(invalid_delete_operation_1.validate().is_err());

        let invalid_delete_operation_2 = Operation {
            action: OperationAction::Delete,
            version: OperationVersion::Default,
            schema: schema_id,
            previous_operations: Some(prev_op_id),
            // DELETE operations must not contain fields
            fields: Some(operation_fields), // Error
        };

        assert!(invalid_delete_operation_2.validate().is_err());
    }

    #[rstest]
    fn encode_and_decode(
        schema_id: SchemaId,
        #[from(random_document_view_id)] prev_op_id: DocumentViewId,
        #[from(random_document_id)] document_id: DocumentId,
    ) {
        // Create test operation
        let mut fields = OperationFields::new();

        // Add one field for every kind of OperationValue
        fields
            .add("username", OperationValue::Text("bubu".to_owned()))
            .unwrap();

        fields.add("height", OperationValue::Float(3.5)).unwrap();

        fields.add("age", OperationValue::Integer(28)).unwrap();

        fields
            .add("is_admin", OperationValue::Boolean(false))
            .unwrap();

        fields
            .add(
                "profile_picture",
                OperationValue::Relation(Relation::new(document_id)),
            )
            .unwrap();

        let operation = Operation::new_update(schema_id, prev_op_id, fields).unwrap();

        assert!(operation.is_update());

        // Encode operation ...
        let encoded = OperationEncoded::try_from(&operation).unwrap();

        // ... and decode it again
        let operation_restored = Operation::try_from(&encoded).unwrap();

        assert_eq!(operation, operation_restored);
    }

    #[rstest]
    fn field_ordering(schema_id: SchemaId) {
        // Create first test operation
        let mut fields = OperationFields::new();
        fields
            .add("a", OperationValue::Text("sloth".to_owned()))
            .unwrap();
        fields
            .add("b", OperationValue::Text("penguin".to_owned()))
            .unwrap();

        let first_operation = Operation::new_create(schema_id.clone(), fields).unwrap();

        // Create second test operation with same values but different order of fields
        let mut second_fields = OperationFields::new();
        second_fields
            .add("b", OperationValue::Text("penguin".to_owned()))
            .unwrap();
        second_fields
            .add("a", OperationValue::Text("sloth".to_owned()))
            .unwrap();

        let second_operation = Operation::new_create(schema_id, second_fields).unwrap();

        assert_eq!(first_operation.to_cbor(), second_operation.to_cbor());
    }

    #[test]
    fn field_iteration() {
        // Create first test operation
        let mut fields = OperationFields::new();
        fields
            .add("a", OperationValue::Text("sloth".to_owned()))
            .unwrap();
        fields
            .add("b", OperationValue::Text("penguin".to_owned()))
            .unwrap();

        let mut field_iterator = fields.iter();

        assert_eq!(
            field_iterator.next().unwrap().1,
            &OperationValue::Text("sloth".to_owned())
        );
        assert_eq!(
            field_iterator.next().unwrap().1,
            &OperationValue::Text("penguin".to_owned())
        );
    }

    #[apply(many_valid_operations)]
    fn many_valid_operations_should_encode(#[case] operation: Operation) {
        assert!(OperationEncoded::try_from(&operation).is_ok())
    }

    #[apply(many_valid_operations)]
    fn it_hashes(#[case] operation: Operation) {
        let mut hash_map = HashMap::new();
        let key_value = "Value identified by a hash".to_string();
        hash_map.insert(&operation, key_value.clone());
        let key_value_retrieved = hash_map.get(&operation).unwrap().to_owned();
        assert_eq!(key_value, key_value_retrieved)
    }
}
