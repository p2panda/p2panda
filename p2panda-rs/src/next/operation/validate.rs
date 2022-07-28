// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::next::document::DocumentViewId;
use crate::next::entry::validate::{validate_log_integrity, validate_payload};
use crate::next::entry::{EncodedEntry, Entry};
use crate::next::operation::error::{ValidateOperationError, VerifiedOperationError};
use crate::next::operation::plain::{PlainFields, PlainOperation};
use crate::next::operation::traits::{Actionable, Schematic};
use crate::next::operation::{
    EncodedOperation, Operation, OperationAction, OperationVersion, VerifiedOperation,
};
use crate::next::schema::validate::{validate_all_fields, validate_only_given_fields};
use crate::next::schema::Schema;
use crate::Human;

pub fn validate_operation_with_entry(
    entry: &Entry,
    entry_encoded: &EncodedEntry,
    skiplink_entry: Option<&Entry>,
    backlink_entry: Option<&Entry>,
    plain_operation: &PlainOperation,
    operation_encoded: &EncodedOperation,
    schema: &Schema,
) -> Result<VerifiedOperation, VerifiedOperationError> {
    // Verify that the entry belongs to this operation
    validate_payload(entry, operation_encoded)?;

    // Verify that the entries links are correct
    validate_log_integrity(entry, skiplink_entry, backlink_entry)?;

    // The operation id is the result of a hashing function over the entry bytes.
    let operation_id = entry_encoded.hash().into();

    // Validate and convert plain operation with the help of a schema
    let operation = validate_operation(plain_operation, schema)?;

    Ok(VerifiedOperation {
        entry: entry.to_owned(),
        operation,
        operation_id,
    })
}

pub fn validate_operation<O: Actionable + Schematic>(
    operation: &O,
    schema: &Schema,
) -> Result<Operation, ValidateOperationError> {
    let previous_operations = operation.previous_operations();
    let fields = operation.fields();

    // Make sure the schema id and given schema matches
    if operation.schema_id() != schema.id() {
        return Err(ValidateOperationError::SchemaNotMatching(
            operation.schema_id().display(),
            schema.id().display(),
        ));
    }

    match operation.action() {
        OperationAction::Create => validate_create_operation(previous_operations, fields, schema),
        OperationAction::Update => validate_update_operation(previous_operations, fields, schema),
        OperationAction::Delete => validate_delete_operation(previous_operations, fields, schema),
    }
}

fn validate_create_operation(
    plain_previous_operations: Option<&DocumentViewId>,
    plain_fields: Option<PlainFields>,
    schema: &Schema,
) -> Result<Operation, ValidateOperationError> {
    if plain_previous_operations.is_some() {
        return Err(ValidateOperationError::UnexpectedPreviousOperations);
    }

    let validated_fields = match plain_fields {
        Some(fields) => validate_all_fields(&fields, schema)?,
        None => return Err(ValidateOperationError::ExpectedFields),
    };

    Ok(Operation {
        version: OperationVersion::V1,
        action: OperationAction::Create,
        schema: schema.to_owned(),
        previous_operations: None,
        fields: Some(validated_fields),
    })
}

fn validate_update_operation(
    plain_previous_operations: Option<&DocumentViewId>,
    plain_fields: Option<PlainFields>,
    schema: &Schema,
) -> Result<Operation, ValidateOperationError> {
    let validated_fields = match plain_fields {
        Some(fields) => validate_only_given_fields(&fields, schema)?,
        None => return Err(ValidateOperationError::ExpectedFields),
    };

    match plain_previous_operations {
        Some(previous_operations) => Ok(Operation {
            version: OperationVersion::V1,
            action: OperationAction::Update,
            schema: schema.to_owned(),
            previous_operations: Some(previous_operations.to_owned()),
            fields: Some(validated_fields),
        }),
        None => Err(ValidateOperationError::ExpectedPreviousOperations),
    }
}

fn validate_delete_operation(
    plain_previous_operations: Option<&DocumentViewId>,
    plain_fields: Option<PlainFields>,
    schema: &Schema,
) -> Result<Operation, ValidateOperationError> {
    if plain_fields.is_some() {
        return Err(ValidateOperationError::UnexpectedFields);
    }

    match plain_previous_operations {
        Some(previous_operations) => Ok(Operation {
            version: OperationVersion::V1,
            action: OperationAction::Delete,
            schema: schema.to_owned(),
            previous_operations: Some(previous_operations.to_owned()),
            fields: None,
        }),
        None => Err(ValidateOperationError::ExpectedPreviousOperations),
    }
}

#[cfg(test)]
mod tests {
    use ciborium::cbor;
    use ciborium::value::{Error, Value};
    use rstest::rstest;
    use rstest_reuse::apply;

    use crate::next::operation::decode::decode_operation;
    use crate::next::operation::plain::PlainOperation;
    use crate::next::operation::EncodedOperation;
    use crate::next::schema::{FieldType, Schema, SchemaId};
    use crate::next::test_utils::constants::{HASH, SCHEMA_ID};
    use crate::next::test_utils::fixtures::{schema_id, Fixture};
    use crate::next::test_utils::templates::version_fixtures;

    use super::validate_operation;

    fn cbor_to_plain(value: Value) -> PlainOperation {
        let mut cbor_bytes = Vec::new();
        ciborium::ser::into_writer(&value, &mut cbor_bytes).unwrap();

        let encoded_operation = EncodedOperation::new(&cbor_bytes);
        decode_operation(&encoded_operation).unwrap()
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

        let plain_operation = cbor_to_plain(cbor.expect("Invalid CBOR value"));
        assert!(validate_operation(&plain_operation, &schema).is_ok());
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
    fn wrong_operation_fields(
        #[from(schema_id)] schema_id: SchemaId,
        #[case] schema_fields: Vec<(&str, FieldType)>,
        #[case] raw_operation: Result<Value, Error>,
        #[case] expected: &str,
    ) {
        let schema = Schema::new(&schema_id, "Some schema description", schema_fields)
            .expect("Could not create schema");

        let plain_operation = cbor_to_plain(raw_operation.expect("Invalid CBOR value"));
        assert_eq!(
            validate_operation(&plain_operation, &schema)
                .err()
                .expect("Expect error")
                .to_string(),
            expected
        );
    }

    #[apply(version_fixtures)]
    fn validate_fixture_operation(#[case] fixture: Fixture) {
        // Validating operation fixture against schema should succeed
        let plain_operation = decode_operation(&fixture.operation_encoded).unwrap();
        assert!(validate_operation(&plain_operation, &fixture.schema).is_ok());
    }
}
