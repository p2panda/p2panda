// SPDX-License-Identifier: AGPL-3.0-or-later

use std::convert::TryFrom;
use std::hash::Hash as StdHash;

use serde::{Deserialize, Serialize};

use crate::hash::Hash;
use crate::operation::{Operation, OperationEncodedError};
use crate::schema::{Schema, SchemaId};
use crate::Validate;

use super::operation::PlainOperation;
use super::OperationError;

/// Operation represented in hex encoded CBOR format.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, StdHash)]
pub struct OperationEncoded(String);

impl OperationEncoded {
    /// Validates and wraps encoded operation string into a new `OperationEncoded` instance.
    pub fn new(value: &str) -> Result<OperationEncoded, OperationEncodedError> {
        let inner = Self(value.to_owned());
        inner.validate()?;
        Ok(inner)
    }

    /// Returns the hash of this operation.
    pub fn hash(&self) -> Hash {
        // Unwrap as we already know that the inner value is valid
        Hash::new_from_bytes(self.to_bytes()).unwrap()
    }

    /// Returns encoded operation as string.
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    /// Decodes hex encoding and returns operation as bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        // Unwrap as we already know that the inner value is valid
        hex::decode(&self.0).unwrap()
    }

    /// Returns payload size (number of bytes) of encoded operation.
    pub fn size(&self) -> u64 {
        // Divide by 2 as every byte is represented by 2 hex chars
        self.0.len() as u64 / 2
    }

    /// Decodes the contained operation with information from the operation's schema definiton.
    pub fn decode(&self, schema: &Schema) -> Result<Operation, OperationEncodedError> {
        let plain_operation: PlainOperation =
            ciborium::de::from_reader(&self.to_bytes()[..]).unwrap();
        Ok(plain_operation.to_operation(schema)?)
    }

    /// Access the operation's schema id by decoding temporarily.
    pub fn schema_id(&self) -> SchemaId {
        let plain_operation: PlainOperation =
            ciborium::de::from_reader(&self.to_bytes()[..]).unwrap();
        plain_operation.schema().clone()
    }
}

/// Returns an encoded version of this operation.
impl TryFrom<&Operation> for OperationEncoded {
    type Error = OperationEncodedError;

    fn try_from(operation: &Operation) -> Result<Self, Self::Error> {
        let encoded = hex::encode(&operation.to_cbor());
        OperationEncoded::new(&encoded)
    }
}

impl Validate for OperationEncoded {
    type Error = OperationEncodedError;

    /// Checks encoded operation value against hex format.
    fn validate(&self) -> Result<(), Self::Error> {
        hex::decode(&self.0).map_err(|_| OperationEncodedError::InvalidHexEncoding)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::convert::TryFrom;

    use rstest::rstest;
    use rstest_reuse::apply;

    use crate::document::DocumentId;
    use crate::operation::{AsOperation, Operation, OperationValue, Relation, RelationList};
    use crate::schema::{FieldType, Schema};
    use crate::test_utils::constants::TEST_SCHEMA_ID;
    use crate::test_utils::fixtures::{
        encoded_create_string, operation, operation_encoded_invalid_relation_fields,
        operation_fields, random_document_id, random_document_view_id, schema, schema_item,
        Fixture,
    };
    use crate::test_utils::templates::version_fixtures;
    use crate::Validate;

    use super::OperationEncoded;

    #[rstest]
    fn validate(encoded_create_string: String) {
        // Invalid hex string
        assert!(OperationEncoded::new("123456789Z").is_err());

        // Valid CREATE operation
        assert!(OperationEncoded::new(&encoded_create_string).is_ok());
    }

    #[rstest]
    fn decode_invalid_relation_fields(
        operation_encoded_invalid_relation_fields: OperationEncoded,
        #[values(schema_item(
            schema(TEST_SCHEMA_ID),
            "",
            vec![
                ("locations", FieldType::Relation(schema(TEST_SCHEMA_ID))
            )]
        ))]
        schema_item: Schema,
    ) {
        let decoded = operation_encoded_invalid_relation_fields.decode(&schema_item);
        assert!(decoded.is_err());
    }

    #[apply(version_fixtures)]
    fn decode(#[case] fixture: Fixture) {
        let operation = fixture.operation_encoded.decode(&fixture.schema).unwrap();
        assert!(operation.validate().is_ok());
        assert!(operation.is_create());

        let fields = operation.fields().unwrap();
        assert_eq!(
            fields.get("name").unwrap(),
            &OperationValue::Text("chess".to_owned())
        );
        assert_eq!(
            fields.get("description").unwrap(),
            &OperationValue::Text("for playing chess".to_owned())
        );
    }

    #[rstest]
    fn encode_decode_all_field_types(
        #[values(schema_item(
            schema(TEST_SCHEMA_ID),
            "",
            vec![
                ("username", FieldType::String),
                ("age", FieldType::Int),
                ("height", FieldType::Float),
                ("is_admin", FieldType::Bool),
                ("profile_picture", FieldType::Relation(schema(TEST_SCHEMA_ID))),
                ("my_friends", FieldType::RelationList(schema(TEST_SCHEMA_ID)))
            ]
        ))]
        schema_item: Schema,
        #[from(random_document_id)] picture_document: DocumentId,
        #[from(random_document_id)] friend_document_1: DocumentId,
        #[from(random_document_id)] friend_document_2: DocumentId,
        #[from(operation)]
        #[with(
            Some(operation_fields(vec![
              ("username", OperationValue::Text("bubu".to_owned())),
              ("age", OperationValue::Integer(28)),
              ("height", OperationValue::Float(3.5)),
              ("is_admin", OperationValue::Boolean(false)),
              ("profile_picture", OperationValue::Relation(Relation::new(picture_document.clone()))),
              ("my_friends", OperationValue::RelationList(RelationList::new(vec![
                  friend_document_1.clone(),
                  friend_document_2.clone(),
              ]))),
            ])),
            Some(random_document_view_id()),
        )]
        update_operation: Operation,
    ) {
        let operation_encoded = OperationEncoded::try_from(&update_operation).unwrap();
        let operation = operation_encoded.decode(&schema_item).unwrap();

        assert!(operation.is_update());
        assert_eq!(operation.schema(), schema_item.id().clone());

        let fields = operation.fields().unwrap();

        assert_eq!(
            fields.get("username").unwrap(),
            &OperationValue::Text("bubu".to_owned())
        );
        assert_eq!(fields.get("age").unwrap(), &OperationValue::Integer(28));
        assert_eq!(fields.get("height").unwrap(), &OperationValue::Float(3.5));
        assert_eq!(
            fields.get("is_admin").unwrap(),
            &OperationValue::Boolean(false)
        );
        assert_eq!(
            fields.get("profile_picture").unwrap(),
            &OperationValue::Relation(Relation::new(picture_document))
        );
        assert_eq!(
            fields.get("my_friends").unwrap(),
            &OperationValue::RelationList(RelationList::new(vec![
                friend_document_1,
                friend_document_2,
            ]))
        );
    }
}
