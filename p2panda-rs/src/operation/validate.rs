// SPDX-License-Identifier: AGPL-3.0-or-later

//! Collection of low-level validation methods for operations.
use crate::document::DocumentViewId;
use crate::entry::traits::{AsEncodedEntry, AsEntry};
use crate::entry::validate::{validate_log_integrity, validate_payload};
use crate::entry::{EncodedEntry, Entry};
use crate::hash::Hash;
use crate::operation::error::{ValidateOperationError, VerifiedOperationError};
use crate::operation::plain::{PlainFields, PlainOperation};
use crate::operation::traits::{Actionable, AsOperation, Schematic};
use crate::operation::{
    EncodedOperation, Operation, OperationAction, OperationVersion, VerifiedOperation,
};
use crate::schema::validate::{validate_all_fields, validate_only_given_fields};
use crate::schema::Schema;
use crate::Human;

/// Main method for complete verification of an operation and entry pair as per Bamboo and p2panda
/// specification.
///
/// Use this method for a complete check of all untrusted, incoming entries and operations. Since
/// this crate does not supply a persistence layer there are some preparations to be done by the
/// implementer to use this method:
///
/// 1. Decode the incoming entry
/// 2. Decode the incoming operation
/// 3. Look up a `Schema` instance (for example in a schema provider) via the schema id you
///    received from the decoded `PlainOperation`
/// 4. Look up `Entry` instances for the back- & skiplinks claimed by the decoded entry
/// 5. Use decoded and encoded data for this method to apply all checks and create a
///    `VerifiedOperation` instance which guarantees authenticity, log integrity, correct operation
///    format, schema validity etc.
///
/// This method applies the following validation steps:
///
/// 1. Correct hexadecimal entry encoding (when using human-readable encoding format) (#E1)
/// 2. Correct Bamboo encoding as per specification (#E2)
/// 3. Check if back- and skiplinks are correctly set for given sequence number (#E3)
/// 4. Verify log-integrity (matching back- & skiplink entries, author, log id) (#E4)
/// 5. Verify signature (#E5)
/// 6. Check if payload matches claimed hash and size (#E6)
/// 7. Correct hexadecimal operation encoding (when using human-readable encoding format) (#OP1)
/// 8. Correct operation format as per specification, including canonic format checks against
///    duplicate and unsorted operation fields (#OP2)
/// 9. Correctly formatted and canonic operation field values, like document view ids (no
///    duplicates, sorted, when no semantic value is given by that) as per specification (#OP3)
/// 10. Operation fields match the claimed schema (#OP4)
///
/// ```text
///                                                                  Look-Up
///
///             ┌────────────┐                       ┌─────┐    ┌─────┐    ┌─────┐
///  bytes ───► │EncodedEntry├────decode_entry()────►│Entry│    │Entry│    │Entry│
///             └──────┬─────┘                       └──┬──┘    └─────┘    └─────┘
///                    │                                │
///                    └───────────────────────────┐    │       Skiplink   Backlink
///                                                │    │          │          │
///             ┌────────────────┐                 │    │          │          │
///  bytes ───► │EncodedOperation├─────────────┐   │    │          │          │
///             └───────┬────────┘             │   │    │          │          │
///                     │                      │   │    │          │          │
///             decode_operation()             │   │    │          │          │
///                     │            Look-Up   │   │    │          │          │
///                     ▼                      │   │    │          │          │
///              ┌──────────────┐    ┌──────┐  │   │    │          │          │
///              │PlainOperation│    │Schema│  │   │    │          │          │
///              └──────┬───────┘    └──┬───┘  │   │    │          │          │
///                     │               │      │   │    │          │          │
///                     │               │      │   │    │          │          │
///                     │               │      │   │    │          │          │
///                     │               │      │   │    │          │          │
///                     │               ▼      ▼   ▼    ▼          ▼          │
///                     └───────────►  validate_operation_with_entry() ◄──────┘
///                                                 │
///                                                 │
///                                                 │
///                                                 │
///                                                 ▼
///                                         ┌─────────────────┐
///                                         │VerifiedOperation│
///                                         └─────────────────┘
/// ```
#[allow(clippy::too_many_arguments)]
pub fn validate_operation_with_entry(
    entry: &Entry,
    entry_encoded: &EncodedEntry,
    skiplink: Option<(&Entry, &Hash)>,
    backlink: Option<(&Entry, &Hash)>,
    plain_operation: &PlainOperation,
    operation_encoded: &EncodedOperation,
    schema: &Schema,
) -> Result<VerifiedOperation, VerifiedOperationError> {
    // Verify that the entry belongs to this operation
    validate_payload(entry, operation_encoded)?;

    // Verify that the entries links are correct
    validate_log_integrity(entry, skiplink, backlink)?;

    // The operation id is the result of a hashing function over the entry bytes.
    let operation_id = entry_encoded.hash().into();

    // Validate and convert plain operation with the help of a schema
    let operation = validate_operation(plain_operation, schema)?;

    Ok(VerifiedOperation {
        id: operation_id,
        public_key: entry.public_key().to_owned(),
        version: AsOperation::version(&operation),
        action: AsOperation::action(&operation),
        schema_id: AsOperation::schema_id(&operation),
        previous: AsOperation::previous(&operation),
        fields: AsOperation::fields(&operation),
    })
}

/// Check the format of an operation-like data type.
///
/// This method checks against:
///
/// 1. Correct operation format (#OP2)
pub fn validate_operation_format<O: Actionable + Schematic>(
    operation: &O,
) -> Result<(), ValidateOperationError> {
    match operation.action() {
        OperationAction::Create => {
            // We don't want to return the fields here so we ignore them.
            let _ = validate_create_operation_format(operation.previous(), operation.fields())?;
            Ok(())
        }
        OperationAction::Update => {
            // We don't want to return the fields here so we ignore them.
            let _ = validate_update_operation_format(operation.previous(), operation.fields())?;
            Ok(())
        }
        OperationAction::Delete => {
            validate_delete_operation_format(operation.previous(), operation.fields())
        }
    }
}

/// Checks the fields and format of an operation-like data type against a schema.
///
/// This method checks against:
///
/// 1. Correct operation format (#OP2)
/// 2. Correct canonic operation field values, like document view ids of pinned relations (no
///    duplicates, sorted) (#OP3)
/// 3. Schema matches the given operation fields (#OP4)
pub fn validate_operation<O: Actionable + Schematic>(
    operation: &O,
    schema: &Schema,
) -> Result<Operation, ValidateOperationError> {
    let previous = operation.previous();
    let fields = operation.fields();

    // Make sure the schema id and given schema matches
    if operation.schema_id() != schema.id() {
        return Err(ValidateOperationError::SchemaNotMatching(
            operation.schema_id().display(),
            schema.id().display(),
        ));
    }

    match operation.action() {
        OperationAction::Create => validate_create_operation(previous, fields, schema),
        OperationAction::Update => validate_update_operation(previous, fields, schema),
        OperationAction::Delete => validate_delete_operation(previous, fields, schema),
    }
}

/// Validate the header fields of a CREATE operation.
///
/// Returns the unwrapped fields which we may wish to validate agains a schema in a
/// following step.
fn validate_create_operation_format(
    plain_previous_operations: Option<&DocumentViewId>,
    plain_fields: Option<PlainFields>,
) -> Result<PlainFields, ValidateOperationError> {
    match (plain_fields, plain_previous_operations) {
        (None, _) => Err(ValidateOperationError::ExpectedFields),
        (Some(_), Some(_)) => Err(ValidateOperationError::UnexpectedPreviousOperations),
        (Some(fields), None) => Ok(fields),
    }
}

/// Validate the header fields of a UPDATE operation.
///
/// Returns the unwrapped fields which we may wish to validate agains a schema in a
/// following step.
fn validate_update_operation_format(
    plain_previous_operations: Option<&DocumentViewId>,
    plain_fields: Option<PlainFields>,
) -> Result<PlainFields, ValidateOperationError> {
    match (plain_fields, plain_previous_operations) {
        (None, _) => Err(ValidateOperationError::ExpectedFields),
        (Some(_), None) => Err(ValidateOperationError::ExpectedPreviousOperations),
        (Some(fields), Some(_)) => Ok(fields),
    }
}

/// Validate the header fields of a DELETE operation.
fn validate_delete_operation_format(
    plain_previous_operations: Option<&DocumentViewId>,
    plain_fields: Option<PlainFields>,
) -> Result<(), ValidateOperationError> {
    match (plain_fields, plain_previous_operations) {
        (Some(_), _) => Err(ValidateOperationError::UnexpectedFields),
        (None, None) => Err(ValidateOperationError::ExpectedPreviousOperations),
        (None, Some(_)) => Ok(()),
    }
}

/// Validates a CREATE operation.
fn validate_create_operation(
    plain_previous_operations: Option<&DocumentViewId>,
    plain_fields: Option<PlainFields>,
    schema: &Schema,
) -> Result<Operation, ValidateOperationError> {
    let fields = validate_create_operation_format(plain_previous_operations, plain_fields)?;
    let validated_fields = validate_all_fields(&fields, schema)?;

    Ok(Operation {
        version: OperationVersion::V1,
        action: OperationAction::Create,
        schema_id: schema.id().to_owned(),
        previous: None,
        fields: Some(validated_fields),
    })
}

/// Validates an UPDATE operation.
fn validate_update_operation(
    plain_previous_operations: Option<&DocumentViewId>,
    plain_fields: Option<PlainFields>,
    schema: &Schema,
) -> Result<Operation, ValidateOperationError> {
    let fields = validate_update_operation_format(plain_previous_operations, plain_fields)?;
    let validated_fields = validate_only_given_fields(&fields, schema)?;

    Ok(Operation {
        version: OperationVersion::V1,
        action: OperationAction::Update,
        schema_id: schema.id().to_owned(),
        previous: plain_previous_operations.cloned(),
        fields: Some(validated_fields),
    })
}

/// Validates a DELETE operation.
fn validate_delete_operation(
    plain_previous_operations: Option<&DocumentViewId>,
    plain_fields: Option<PlainFields>,
    schema: &Schema,
) -> Result<Operation, ValidateOperationError> {
    validate_delete_operation_format(plain_previous_operations, plain_fields)?;

    Ok(Operation {
        version: OperationVersion::V1,
        action: OperationAction::Delete,
        schema_id: schema.id().to_owned(),
        previous: plain_previous_operations.cloned(),
        fields: None,
    })
}

#[cfg(test)]
mod tests {
    use ciborium::cbor;
    use ciborium::value::{Error, Value};
    use rstest::rstest;
    use rstest_reuse::apply;

    use crate::document::{DocumentId, DocumentViewId};
    use crate::operation::decode::decode_operation;
    use crate::operation::plain::PlainOperation;
    use crate::operation::{EncodedOperation, OperationAction, OperationBuilder};
    use crate::schema::{FieldType, Schema, SchemaId};
    use crate::test_utils::constants::{HASH, SCHEMA_ID};
    use crate::test_utils::fixtures::{document_id, document_view_id, schema, schema_id, Fixture};
    use crate::test_utils::templates::version_fixtures;

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
                .expect_err("Expect error")
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

    #[rstest]
    fn operation_schema_validation(
        #[with(vec![
                ("firstname".into(), FieldType::String),
                ("year".into(), FieldType::Integer),
                ("is_cute".into(), FieldType::Boolean),
                ("address".into(), FieldType::Relation(schema_id(SCHEMA_ID))),
            ])]
        schema: Schema,
        document_id: DocumentId,
        document_view_id: DocumentViewId,
    ) {
        // Operation matches schema
        let operation = OperationBuilder::new(schema.id())
            .fields(&[
                ("firstname", "Peter".into()),
                ("year", 2020.into()),
                ("is_cute", false.into()),
                ("address", document_id.clone().into()),
            ])
            .build()
            .unwrap();

        assert!(validate_operation(&operation, &schema).is_ok());

        // Field ordering does not matter in builder
        let operation = OperationBuilder::new(schema.id())
            .fields(&[
                ("address", document_id.clone().into()),
                ("is_cute", false.into()),
                ("year", 2020.into()),
                ("firstname", "Peter".into()),
            ])
            .build()
            .unwrap();

        assert!(validate_operation(&operation, &schema).is_ok());

        // Field missing
        let operation = OperationBuilder::new(schema.id())
            .fields(&[
                ("firstname", "Peter".into()),
                ("is_cute", false.into()),
                ("address", document_id.clone().into()),
            ])
            .build()
            .unwrap();

        assert!(validate_operation(&operation, &schema).is_err());

        // Invalid type
        let operation = OperationBuilder::new(schema.id())
            .fields(&[
                ("firstname", "Peter".into()),
                ("year", "2020".into()),
                ("is_cute", false.into()),
                ("address", document_id.clone().into()),
            ])
            .build()
            .unwrap();

        assert!(validate_operation(&operation, &schema).is_err());

        // Correct UPDATE operation matching schema
        let operation = OperationBuilder::new(schema.id())
            .action(OperationAction::Update)
            .previous(&document_view_id)
            .fields(&[("address", document_id.into())])
            .build()
            .unwrap();

        assert!(validate_operation(&operation, &schema).is_ok());
    }
}
