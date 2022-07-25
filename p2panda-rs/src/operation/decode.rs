// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::operation::{
    verify_schema_and_convert, EncodedOperation, Operation, RawOperation, RawOperationError,
};
use crate::schema::Schema;

pub fn decode_operation(
    encoded_operation: &EncodedOperation,
    schema: &Schema,
) -> Result<Operation, RawOperationError> {
    let bytes = encoded_operation.as_bytes();

    let raw_operation: RawOperation =
        ciborium::de::from_reader(bytes).map_err(|err| match err {
            ciborium::de::Error::Io(err) => RawOperationError::DecoderFailed(err.to_string()),
            ciborium::de::Error::Syntax(err) => {
                RawOperationError::InvalidCBOREncoding(err.to_string())
            }
            ciborium::de::Error::Semantic(_, err) => {
                RawOperationError::InvalidEncoding(err.to_string())
            }
            ciborium::de::Error::RecursionLimitExceeded => {
                RawOperationError::DecoderFailed("Recursion limit exceeded".into())
            }
        })?;

    let operation = verify_schema_and_convert(&raw_operation, schema)?;

    Ok(operation)
}

#[cfg(test)]
mod tests {
    use ciborium::cbor;
    use ciborium::value::{Error, Value};
    use rstest::rstest;

    use crate::operation::EncodedOperation;
    use crate::schema::{FieldType, Schema, SchemaId};
    use crate::test_utils::constants::{HASH, SCHEMA_ID};
    use crate::test_utils::fixtures::schema_id;

    use super::decode_operation;

    fn encode_cbor(value: Value) -> EncodedOperation {
        let mut cbor_bytes = Vec::new();
        ciborium::ser::into_writer(&value, &mut cbor_bytes).unwrap();
        EncodedOperation::new(&cbor_bytes)
    }

    #[rstest]
    #[case(
        vec![
            ("country", FieldType::Relation(schema_id.clone())),
            ("national_dish", FieldType::Text),
            ("vegan_friendly", FieldType::Boolean),
            ("yummyness", FieldType::Integer),
            ("yumsimumsiness", FieldType::Float),
        ],
        cbor!([
            1, 0, SCHEMA_ID,
            {
                "country" => HASH,
                "national_dish" => "Pumpkin",
                "vegan_friendly" => true,
                "yummyness" => 8,
                "yumsimumsiness" => 7.2,
            },
        ]),
    )]
    fn valid_operations(
        #[from(schema_id)] schema_id: SchemaId,
        #[case] schema_fields: Vec<(&str, FieldType)>,
        #[case] cbor: Result<Value, Error>,
    ) {
        let schema = Schema::new(&schema_id, "Some schema description", schema_fields)
            .expect("Could not create schema");

        let encoded_operation = encode_cbor(cbor.expect("Invalid CBOR value"));
        assert!(decode_operation(&encoded_operation, &schema).is_ok());
    }

    #[rstest]
    #[case::incomplete_hash(
        vec![
            ("country", FieldType::Relation(schema_id.clone())),
        ],
        cbor!([
            1, 0, SCHEMA_ID,
            {
                "country" => "0020",
            },
        ]),
        "field 'country' does not match schema: invalid hash length 2 bytes, expected 34 bytes"
    )]
    #[case::invalid_hex_encoding(
        vec![
            ("country", FieldType::Relation(schema_id.clone())),
        ],
        cbor!([
            1, 0, SCHEMA_ID,
            {
                "country" => "xyz",
            },
        ]),
        "field 'country' does not match schema: invalid hex encoding in hash string"
    )]
    #[case::missing_field(
        vec![
            ("national_dish", FieldType::Text),
        ],
        cbor!([
            1, 0, SCHEMA_ID,
            {
                "vegan_friendly" => true,
            },
        ]),
        "field 'vegan_friendly' does not match schema: expected field name 'national_dish'"
    )]
    #[case::unordered_field_names(
        vec![
            ("a", FieldType::Text),
            ("b", FieldType::Text),
        ],
        cbor!([
            1, 0, SCHEMA_ID,
            {
                "b" => "test",
                "a" => "test",
            },
        ]),
        "encountered unsorted field name: 'a' should be before 'b'"
    )]
    #[case::duplicate_field_names(
        vec![
            ("a", FieldType::Text),
        ],
        cbor!([
            1, 0, SCHEMA_ID,
            {
                "a" => "test",
                "a" => "test",
            },
        ]),
        "encountered duplicate field key 'a'"
    )]
    fn wrong_operation_fields(
        #[from(schema_id)] schema_id: SchemaId,
        #[case] schema_fields: Vec<(&str, FieldType)>,
        #[case] raw_operation: Result<Value, Error>,
        #[case] expected: &str,
    ) {
        let schema = Schema::new(&schema_id, "Some schema description", schema_fields)
            .expect("Could not create schema");

        let bytes = encode_cbor(raw_operation.expect("Invalid CBOR value"));
        assert_eq!(
            decode_operation(&bytes, &schema)
                .err()
                .expect("Expect error")
                .to_string(),
            expected
        );
    }
}
