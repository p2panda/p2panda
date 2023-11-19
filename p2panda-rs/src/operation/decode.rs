// SPDX-License-Identifier: AGPL-3.0-or-later

//! Methods to decode an operation and validate it against its claimed schema.
//!
//! To derive an `Operation` from bytes or a hexadecimal string, use the `EncodedOperation` struct
//! and apply the `decode_operation` method, which returns a `PlainOperation` instance, allowing
//! you to access the "header" information of the operation, like the schema id, operation action
//! and version.
//!
//! ```text
//!             ┌────────────────┐                             ┌──────────────┐
//!  bytes ───► │EncodedOperation│ ────decode_operation()────► │PlainOperation│
//!             └────────────────┘                             └──────────────┘
//! ```
//!
//! Move on to `operation::validate` for methods to check the `PlainOperation` against the claimed
//! `Schema` instance to eventually get the `Operation` instance.
use crate::operation::error::DecodeOperationError;
use crate::operation::plain::PlainOperation;
use crate::operation::EncodedOperation;

/// Method to decode an operation.
///
/// This method validates against:
///
/// 1. Correct canonic operation format as per specification (#OP2)
/// 2. Ensures canonic field values format (sorted arrays, no duplicates) (#OP3)
pub fn decode_operation(
    encoded_operation: &EncodedOperation,
) -> Result<PlainOperation, DecodeOperationError> {
    let bytes = encoded_operation.into_bytes();

    let plain_operation: PlainOperation =
        ciborium::de::from_reader(&bytes[..]).map_err(|err| match err {
            ciborium::de::Error::Io(err) => DecodeOperationError::DecoderIOFailed(err.to_string()),
            ciborium::de::Error::Syntax(pos) => DecodeOperationError::InvalidCBOREncoding(pos),
            ciborium::de::Error::Semantic(_, err) => DecodeOperationError::InvalidEncoding(err),
            ciborium::de::Error::RecursionLimitExceeded => {
                DecodeOperationError::RecursionLimitExceeded
            }
        })?;

    Ok(plain_operation)
}

#[cfg(test)]
mod tests {
    use ciborium::cbor;
    use ciborium::value::{Error, Value};
    use rstest::rstest;
    use rstest_reuse::apply;

    use crate::operation::EncodedOperation;
    use crate::serde::{hex_string_to_bytes, serialize_value};
    use crate::test_utils::constants::{HASH, SCHEMA_ID};
    use crate::test_utils::fixtures::Fixture;
    use crate::test_utils::templates::version_fixtures;

    use super::decode_operation;

    fn encode_cbor(value: Value) -> EncodedOperation {
        EncodedOperation::new(&serialize_value(Ok(value)))
    }

    #[rstest]
    #[case::duplicate_field_names(
        cbor!([
            1, 0, SCHEMA_ID,
            {
                "a" => "test",
                "a" => "test",
            },
        ]),
        "encountered duplicate field key 'a'"
    )]
    #[case::unordered_field_names(
        cbor!([
            1, 0, SCHEMA_ID,
            {
                "b" => "test",
                "a" => "test",
            },
        ]),
        "encountered unsorted field name: 'a' should be before 'b'"
    )]
    fn wrong_canonic_encoding(#[case] raw_operation: Result<Value, Error>, #[case] expected: &str) {
        let bytes = encode_cbor(raw_operation.expect("Invalid CBOR value"));
        assert_eq!(
            decode_operation(&bytes)
                .expect_err("Expect error")
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
        cbor!([1, "this is not an action", SCHEMA_ID, { "is_cute" => true }]),
        "invalid type: string, expected integer"
    )]
    #[case::unsupported_action(
        cbor!([1, 100, SCHEMA_ID, { "is_cute" => true }]),
        "unknown operation action 100"
    )]
    #[case::missing_schema_id(
        cbor!([1, 0]),
        "missing schema id field in operation format"
    )]
    #[case::invalid_schema_id_incomplete(
        cbor!([1, 0, "venue_0020", { "name" => "Panda" }]),
        "encountered invalid hash while parsing application schema id: invalid hash length 2 bytes, expected 34 bytes"
    )]
    #[case::invalid_schema_id_hex(
        cbor!([1, 0, "this is not a hash", { "name" => "Panda" }]),
        "malformed schema id `this is not a hash`: doesn't contain an underscore"
    )]
    #[case::invalid_schema_id_name_missing(
        cbor!([1, 0, HASH, { "name" => "Panda" }]),
        "malformed schema id `0020b177ec1bf26dfb3b7010d473e6d44713b29b765b99c6e60ecbfae742de496543`: doesn't contain an underscore"
    )]
    #[case::non_canonic_schema_id_unsorted(
        cbor!(
            [
              1,
              0,
              "venue_00209a75d6f1440c188fa52555c8cdd60b3988e468e1db2e469b7d4425a225eba8ec_0020175257cbf0259eac4b4832695134ac9b2858d7c7cb6c199af8cf22a1db2dbc45",
              { "name" => "Panda" }
            ]
        ),
        "encountered invalid document view id while parsing application schema id: expected sorted operation ids in document view id"
    )]
    #[case::non_canonic_schema_id_duplicates(
        cbor!(
            [
              1,
              0,
              "venue_00209a75d6f1440c188fa52555c8cdd60b3988e468e1db2e469b7d4425a225eba8ec_00209a75d6f1440c188fa52555c8cdd60b3988e468e1db2e469b7d4425a225eba8ec",
              { "name" => "Panda" }
            ]
        ),
        "encountered invalid document view id while parsing application schema id: expected sorted operation ids in document view id"
    )]
    #[case::invalid_previous_operations_hex(
        cbor!([1, 2, SCHEMA_ID, [serde_bytes::ByteBuf::from("_correct_num_of_bytes_but_not_hex_")]]),
        "can not decode YASMF BLAKE3 hash"
    )]
    #[case::invalid_previous_operations_incomplete(
        cbor!([1, 2, SCHEMA_ID, [hex_string_to_bytes("0020")]]),
        "invalid hash length 2 bytes, expected 34 bytes"
    )]
    #[case::invalid_previous_operations_array(
        cbor!([1, 2, SCHEMA_ID, {}]),
        "invalid type: map, expected document view id as array or in string representation"
    )]
    #[case::non_canonic_previous_operations_unsorted(
        cbor!([1, 2, SCHEMA_ID, [
            hex_string_to_bytes("0020f0b5a6e87e1a039f18857ee1c0792fd24fe1b3ad962c8950cba6c10290b619e3"),
            hex_string_to_bytes("002044ed67b81c26cf2f7c3eb908cf4620d18a0ac3d79bf70d64b2f02d965466a8f0")
        ]]),
        "expected sorted operation ids in document view id"
    )]
    #[case::non_canonic_previous_operations_duplicates(
        cbor!([1, 2, SCHEMA_ID, [
              hex_string_to_bytes("002044ed67b81c26cf2f7c3eb908cf4620d18a0ac3d79bf70d64b2f02d965466a8f0"),
              hex_string_to_bytes("0020f0b5a6e87e1a039f18857ee1c0792fd24fe1b3ad962c8950cba6c10290b619e3"),
              hex_string_to_bytes("002044ed67b81c26cf2f7c3eb908cf4620d18a0ac3d79bf70d64b2f02d965466a8f0")
        ]]),
        "expected sorted operation ids in document view id"
    )]
    #[case::invalid_fields_key_type_1(
        cbor!([1, 0, SCHEMA_ID, { 12 => "Panda" }]),
        "invalid type: integer `12`, expected string"
    )]
    #[case::invalid_fields_key_type_2(
        cbor!([1, 0, SCHEMA_ID, { "a" => "value", "b" => { "nested" => "wrong " }}]),
        "error deserializing plain value: data did not match any variant of untagged enum PlainValue"
    )]
    #[case::invalid_fields_value_type(
        cbor!([1, 0, SCHEMA_ID, { "some" => { "nested" => "map" } }]),
        "error deserializing plain value: data did not match any variant of untagged enum PlainValue"
    )]
    #[case::missing_schema_create(
        cbor!([1, 0, { "is_cute" => true }]),
        "invalid type: map, expected schema id as string"
    )]
    #[case::missing_schema_update(
        cbor!([1, 1, [hex_string_to_bytes(HASH), hex_string_to_bytes(HASH)], { "is_cute" => true }]),
        "invalid type: sequence, expected schema id as string"
    )]
    #[case::missing_schema_delete(
        cbor!([1, 2, [hex_string_to_bytes(HASH)]]),
        "invalid type: sequence, expected schema id as string"
    )]
    #[case::invalid_previous_operations_create(
        cbor!([1, 0, SCHEMA_ID, [hex_string_to_bytes(HASH)], { "is_cute" => true }]),
        "invalid type: sequence, expected map"
    )]
    #[case::missing_previous_operations_update(
        cbor!([1, 1, SCHEMA_ID, { "is_cute" => true }]),
        "invalid type: map, expected document view id as array or in string representation"
    )]
    #[case::missing_previous_operations_delete(
        cbor!([1, 2, SCHEMA_ID ]),
        "missing previous for this operation action"
    )]
    #[case::missing_fields_create(
        cbor!([1, 0, SCHEMA_ID ]),
        "missing fields for this operation action"
    )]
    #[case::missing_fields_update(
        cbor!([1, 1, SCHEMA_ID, [hex_string_to_bytes(HASH)]]),
        "missing fields for this operation action"
    )]
    #[case::invalid_fields_delete(
        cbor!([1, 2, SCHEMA_ID, [hex_string_to_bytes(HASH)], { "is_wrong" => true }]),
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
                .expect_err("Expect error")
                .to_string(),
            expected
        );
    }

    #[apply(version_fixtures)]
    fn decode_fixture_operation(#[case] fixture: Fixture) {
        // Decoding operation fixture should succeed
        assert!(decode_operation(&fixture.operation_encoded).is_ok());
    }
}
