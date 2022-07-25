// SPDX-License-Identifier: AGPL-3.0-or-later

use std::fmt::Display;
use std::hash::Hash as StdHash;

use serde::{Deserialize, Serialize};

use crate::hash::Hash;
use crate::operation::{encode_operation, encode_raw_operation, Operation, RawOperation};
use crate::serde::{deserialize_hex, serialize_hex};

/// Wrapper type for operation bytes.
#[derive(Clone, Debug, PartialEq, Eq, StdHash, Serialize, Deserialize)]
pub struct EncodedOperation(
    #[serde(serialize_with = "serialize_hex", deserialize_with = "deserialize_hex")] Vec<u8>,
);

impl EncodedOperation {
    /// Returns the hash of this operation.
    pub fn hash(&self) -> Hash {
        Hash::new_from_bytes(self.0)
    }

    /// Returns operation as bytes.
    pub fn into_bytes(&self) -> Vec<u8> {
        self.0
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0[..]
    }

    /// Returns payload size (number of bytes) of encoded operation.
    pub fn size(&self) -> u64 {
        self.0.len() as u64
    }
}

impl Display for EncodedOperation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", hex::encode(self.0))
    }
}

impl From<&Operation> for EncodedOperation {
    fn from(operation: &Operation) -> Self {
        let bytes = encode_operation(operation).unwrap();
        Self(bytes)
    }
}

impl From<&RawOperation> for EncodedOperation {
    fn from(raw_operation: &RawOperation) -> Self {
        let bytes = encode_raw_operation(raw_operation).unwrap();
        Self(bytes)
    }
}

#[cfg(test)]
impl EncodedOperation {
    pub fn new(bytes: &[u8]) -> EncodedOperation {
        Self(bytes.to_owned())
    }

    pub fn from_str(value: &str) -> EncodedOperation {
        let bytes = hex::decode(value).expect("invalid hexadecimal value");
        Self(bytes)
    }
}

// @TODO: Move this to decode, encode tests
/* #[cfg(test)]
mod tests {
    use std::convert::TryFrom;

    use rstest::rstest;
    use rstest_reuse::apply;

    use crate::document::DocumentId;
    use crate::operation::{AsOperation, Operation, OperationValue, Relation, RelationList};
    use crate::schema::SchemaId;
    use crate::test_utils::fixtures::{
        encoded_create_string, operation, operation_encoded_invalid_relation_fields,
        operation_fields, random_document_id, random_document_view_id, schema_id, Fixture,
    };
    use crate::test_utils::templates::version_fixtures;
    use crate::Validate;

    use super::EncodedOperation;

    #[rstest]
    fn validate(encoded_create_string: String) {
        // Invalid hex string
        assert!(EncodedOperation::new("123456789Z").is_err());

        // Valid CREATE operation
        assert!(EncodedOperation::new(&encoded_create_string).is_ok());
    }

    #[rstest]
    fn decode_invalid_relation_fields(operation_encoded_invalid_relation_fields: EncodedOperation) {
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
        schema_id: SchemaId,
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
        let operation_encoded = EncodedOperation::try_from(&update_operation).unwrap();
        let operation = Operation::try_from(&operation_encoded).unwrap();

        assert!(operation.is_update());
        assert_eq!(operation.schema(), schema_id);

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
} */
