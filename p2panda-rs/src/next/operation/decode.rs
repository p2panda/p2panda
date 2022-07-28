// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::next::operation::error::DecodeOperationError;
use crate::next::operation::plain::PlainOperation;
use crate::next::operation::validate::validate_operation;
use crate::next::operation::{EncodedOperation, Operation};
use crate::next::schema::Schema;

pub fn decode_operation(
    encoded_operation: &EncodedOperation,
) -> Result<PlainOperation, DecodeOperationError> {
    let bytes = encoded_operation.as_bytes();

    let plain_operation: PlainOperation =
        ciborium::de::from_reader(bytes).map_err(|err| match err {
            ciborium::de::Error::Io(err) => DecodeOperationError::DecoderIOFailed(err.to_string()),
            ciborium::de::Error::Syntax(pos) => DecodeOperationError::InvalidCBOREncoding(pos),
            ciborium::de::Error::Semantic(_, err) => {
                DecodeOperationError::InvalidEncoding(err.to_string())
            }
            ciborium::de::Error::RecursionLimitExceeded => {
                DecodeOperationError::RecursionLimitExceeded
            }
        })?;

    Ok(plain_operation)
}

pub fn decode_and_validate_operation(
    encoded_operation: &EncodedOperation,
    schema: &Schema,
) -> Result<Operation, DecodeOperationError> {
    let plain_operation = decode_operation(&encoded_operation)?;
    let operation = validate_operation(&plain_operation, &schema)?;
    Ok(operation)
}

#[cfg(test)]
mod tests {
    use ciborium::cbor;
    use ciborium::value::{Error, Value};
    use rstest::rstest;
    use rstest_reuse::apply;

    use crate::next::operation::EncodedOperation;
    use crate::next::schema::{FieldType, Schema, SchemaId};
    use crate::next::test_utils::constants::{HASH, SCHEMA_ID};
    use crate::next::test_utils::fixtures::{schema_id, Fixture};
    use crate::next::test_utils::templates::version_fixtures;

    use super::{decode_and_validate_operation, decode_operation};

    fn encode_cbor(value: Value) -> EncodedOperation {
        let mut cbor_bytes = Vec::new();
        ciborium::ser::into_writer(&value, &mut cbor_bytes).unwrap();
        EncodedOperation::new(&cbor_bytes)
    }

    #[rstest]
    #[case(
        vec![
            ("country", FieldType::Relation(schema_id.clone())),
            ("national_dish", FieldType::String),
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
        assert!(decode_and_validate_operation(&encoded_operation, &schema).is_ok());
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
            ("national_dish", FieldType::String),
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
            ("a", FieldType::String),
            ("b", FieldType::String),
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
            ("a", FieldType::String),
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
            decode_and_validate_operation(&bytes, &schema)
                .err()
                .expect("Expect error")
                .to_string(),
            expected
        );
    }

    #[rstest]
    #[case::garbage_1(
        cbor!({ "action" => "oldschool", "version" => 1 }),
        "invalid type: map, expected array"
    )]
    #[case::garbage_2(
        cbor!([1, 2, 3, 4, 5, 6, 7, 8, 9]),
        "invalid type: integer `3`, expected schema id as string"
    )]
    #[case::garbage_3(
        cbor!("01"),
        "invalid type: string, expected array"
    )]
    #[case::missing_version(
        cbor!([]),
        "missing version field in operation format"
    )]
    #[case::invalid_version(
        cbor!(["this is not a version", 0, SCHEMA_ID, { "name" => "Panda" }]),
        "invalid type: string, expected integer"
    )]
    #[case::unsupported_version_1(
        cbor!([100, 0, SCHEMA_ID, { "name" => "Panda" }]),
        "unsupported operation version 100"
    )]
    #[case::unsupported_version_2(
        cbor!([0, 0, SCHEMA_ID, { "name" => "Panda" }]),
        "unsupported operation version 0"
    )]
    #[case::missing_action(
        cbor!([1]),
        "missing action field in operation format"
    )]
    #[case::invalid_action(
        cbor!([1, "this is not an action", SCHEMA_ID, { "is_cute" => true } ]),
        "invalid type: string, expected integer"
    )]
    #[case::unsupported_action(
        cbor!([1, 100, SCHEMA_ID, { "is_cute" => true } ]),
        "unknown operation action 100"
    )]
    #[case::missing_schema_id(
        cbor!([1, 0]),
        "missing schema id field in operation format"
    )]
    #[case::invalid_schema_id_incomplete(
        cbor!([1, 0, "venue_0020", { "name" => "Panda" } ]),
        "encountered invalid hash while parsing application schema id: invalid hash length 2 bytes, expected 34 bytes"
    )]
    #[case::invalid_schema_id_hex(
        cbor!([1, 0, "this is not a hash", { "name" => "Panda" } ]),
        "malformed schema id `this is not a hash`: doesn't contain an underscore"
    )]
    #[case::invalid_schema_id_name_missing(
        cbor!([1, 0, HASH, { "name" => "Panda" } ]),
        "malformed schema id `0020b177ec1bf26dfb3b7010d473e6d44713b29b765b99c6e60ecbfae742de496543`: doesn't contain an underscore"
    )]
    #[case::invalid_previous_operations_hex(
        cbor!([1, 2, SCHEMA_ID, ["this is not a hash"] ]),
        "Error parsing document view id at position 0: invalid hex encoding in hash string"
    )]
    #[case::invalid_previous_operations_incomplete(
        cbor!([1, 2, SCHEMA_ID, ["0020"] ]),
        "Error parsing document view id at position 0: invalid hash length 2 bytes, expected 34 bytes"
    )]
    #[case::invalid_previous_operations_array(
        cbor!([1, 2, SCHEMA_ID, {} ]),
        "invalid type: map, expected array"
    )]
    #[case::invalid_fields_key_type_1(
        cbor!([1, 0, SCHEMA_ID, { 12 => "Panda" } ]),
        "invalid type: integer `12`, expected string"
    )]
    #[case::invalid_fields_key_type_2(
        cbor!([1, 0, SCHEMA_ID, { "a" => "value", "b" => { "nested" => "wrong "} } ]),
        "data did not match any variant of untagged enum PlainValue"
    )]
    #[case::invalid_fields_value_type(
        cbor!([1, 0, SCHEMA_ID, { "some" => { "nested" => "map" } } ]),
        "data did not match any variant of untagged enum PlainValue"
    )]
    #[case::missing_schema_create(
        cbor!([1, 0, { "is_cute" => true } ]),
        "invalid type: map, expected schema id as string"
    )]
    #[case::missing_schema_update(
        cbor!([1, 1, [HASH, HASH], { "is_cute" => true } ]),
        "invalid type: sequence, expected schema id as string"
    )]
    #[case::missing_schema_delete(
        cbor!([1, 2, [HASH] ]),
        "invalid type: sequence, expected schema id as string"
    )]
    #[case::invalid_previous_operations_create(
        cbor!([1, 0, SCHEMA_ID, [HASH], { "is_cute" => true } ]),
        "invalid type: sequence, expected map"
    )]
    #[case::missing_previous_operations_update(
        cbor!([1, 1, SCHEMA_ID, { "is_cute" => true } ]),
        "invalid type: map, expected array"
    )]
    #[case::missing_previous_operations_delete(
        cbor!([1, 2, SCHEMA_ID ]),
        "missing previous_operations field for this operation action"
    )]
    #[case::missing_fields_create(
        cbor!([1, 0, SCHEMA_ID ]),
        "missing fields for this operation action"
    )]
    #[case::missing_fields_update(
        cbor!([1, 1, SCHEMA_ID, [HASH] ]),
        "missing fields for this operation action"
    )]
    #[case::invalid_fields_delete(
        cbor!([1, 2, SCHEMA_ID, [HASH], { "is_wrong" => true }]),
        "too many items for this operation action"
    )]
    #[case::too_many_items_create(
        cbor!([1, 0, SCHEMA_ID, { "is_cute" => true }, { "is_cute" => true }]),
        "too many items for this operation action"
    )]
    fn wrong_operation_format(#[case] raw_operation: Result<Value, Error>, #[case] expected: &str) {
        let bytes = encode_cbor(raw_operation.expect("Invalid CBOR value"));
        assert_eq!(
            decode_operation(&bytes)
                .err()
                .expect("Expect error")
                .to_string(),
            expected
        );
    }

    #[apply(version_fixtures)]
    fn decode_fixture_operation(#[case] fixture: Fixture) {
        // Decoding and validating operation fixture should succeed
        assert!(decode_and_validate_operation(&fixture.operation_encoded, &fixture.schema).is_ok());
    }
}
