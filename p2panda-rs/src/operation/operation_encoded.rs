// SPDX-License-Identifier: AGPL-3.0-or-later

use std::convert::TryFrom;
use std::hash::Hash as StdHash;

use serde::{Deserialize, Serialize};

use crate::hash::Hash;
use crate::operation::{Operation, OperationEncodedError};
#[cfg(not(target_arch = "wasm32"))]
use crate::schema::{validate_schema, OPERATION_SCHEMA};
use crate::Validate;

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
}

/// Returns an encoded version of this operation.
impl TryFrom<&Operation> for OperationEncoded {
    type Error = OperationEncodedError;

    fn try_from(operation: &Operation) -> Result<Self, Self::Error> {
        let encoded = hex::encode(&operation.to_cbor());
        OperationEncoded::new(&encoded)
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Validate for OperationEncoded {
    type Error = OperationEncodedError;

    /// Checks encoded operation value against hex format and CDDL schema.
    fn validate(&self) -> Result<(), Self::Error> {
        // Validate hex encoding
        let bytes = hex::decode(&self.0).map_err(|_| OperationEncodedError::InvalidHexEncoding)?;

        // Validate CDDL schema
        validate_schema(OPERATION_SCHEMA, bytes)?;

        Ok(())
    }
}

#[cfg(target_arch = "wasm32")]
impl Validate for OperationEncoded {
    type Error = OperationEncodedError;

    /// Checks encoded operation value against hex format.
    ///
    /// Skips CDDL schema validation as this is not supported for wasm targets. See:
    /// https://github.com/anweiss/cddl/issues/83
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
    use crate::schema::SchemaId;
    use crate::test_utils::fixtures::templates::version_fixtures;
    use crate::test_utils::fixtures::{
        encoded_create_string, fields, operation_encoded_invalid_relation_fields,
        random_document_id, random_hash, schema, update_operation, Fixture,
    };
    use crate::Validate;

    use super::OperationEncoded;

    #[rstest]
    fn validate(encoded_create_string: String) {
        // Invalid hex string
        assert!(OperationEncoded::new("123456789Z").is_err());

        // Invalid operation
        assert!(OperationEncoded::new("68656c6c6f2062616d626f6f21").is_err());

        // Valid CREATE operation
        assert!(OperationEncoded::new(&encoded_create_string).is_ok());
    }

    #[rstest]
    fn decode_invalid_relation_fields(operation_encoded_invalid_relation_fields: OperationEncoded) {
        let operation = Operation::try_from(&operation_encoded_invalid_relation_fields).unwrap();
        assert!(operation.validate().is_err());
    }

    #[apply(version_fixtures)]
    fn decode(#[case] fixture: Fixture) {
        let operation = Operation::try_from(&fixture.operation_encoded).unwrap();
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
        schema: SchemaId,
        #[from(random_document_id)] picture_document: DocumentId,
        #[from(random_document_id)] friend_document_1: DocumentId,
        #[from(random_document_id)] friend_document_2: DocumentId,
        #[with(
            // Schema hash
            schema.clone(),
            // Previous operations
            vec![random_hash()],
            // Operation fields
            fields(vec![
              ("username", OperationValue::Text("bubu".to_owned())),
              ("age", OperationValue::Integer(28)),
              ("height", OperationValue::Float(3.5)),
              ("is_admin", OperationValue::Boolean(false)),
              ("profile_picture", OperationValue::Relation(Relation::new(picture_document.clone()))),
              ("my_friends", OperationValue::RelationList(RelationList::new(vec![
                  friend_document_1.clone(),
                  friend_document_2.clone(),
              ]))),
            ])
        )]
        update_operation: Operation,
    ) {
        let operation_encoded = OperationEncoded::try_from(&update_operation).unwrap();
        let operation = Operation::try_from(&operation_encoded).unwrap();

        assert!(operation.is_update());
        assert_eq!(operation.schema(), schema);

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
