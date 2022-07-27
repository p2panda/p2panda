// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::document::DocumentViewId;
use crate::entry::validate::{validate_log_integrity, validate_payload};
use crate::entry::{EncodedEntry, Entry};
use crate::operation::error::{ValidateOperationError, VerifiedOperationError};
use crate::operation::plain::{PlainFields, PlainOperation};
use crate::operation::traits::{Actionable, Schematic};
use crate::operation::{
    EncodedOperation, Operation, OperationAction, OperationVersion, VerifiedOperation,
};
use crate::schema::validate::{validate_all_fields, validate_only_given_fields};
use crate::schema::Schema;

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
    validate_payload(&entry, &operation_encoded)?;

    // Verify that the entries links are correct
    validate_log_integrity(&entry, &skiplink_entry, &backlink_entry)?;

    // The operation id is the result of a hashing function over the entry bytes.
    let operation_id = entry_encoded.hash().into();

    // Validate and convert plain operation with the help of a schema
    let operation = validate_operation(&plain_operation, &schema)?;

    Ok(VerifiedOperation {
        entry,
        operation,
        operation_id,
    })
}

pub fn validate_operation<O: Actionable + Schematic>(
    operation: O,
    schema: &Schema,
) -> Result<Operation, ValidateOperationError> {
    let previous_operations = operation.previous_operations();
    let fields = operation.fields();

    match operation.action() {
        OperationAction::Create => validate_create_operation(previous_operations, fields, schema),
        OperationAction::Update => validate_update_operation(previous_operations, fields, schema),
        OperationAction::Delete => validate_delete_operation(previous_operations, fields, schema),
    }
}

fn validate_create_operation(
    plain_previous_operations: Option<&DocumentViewId>,
    plain_fields: Option<&PlainFields>,
    schema: &Schema,
) -> Result<Operation, ValidateOperationError> {
    if plain_previous_operations.is_some() {
        return Err(ValidateOperationError::UnexpectedPreviousOperations);
    }

    let validated_fields = match plain_fields {
        Some(fields) => validate_all_fields(fields, schema)?,
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
    plain_fields: Option<&PlainFields>,
    schema: &Schema,
) -> Result<Operation, ValidateOperationError> {
    let validated_fields = match plain_fields {
        Some(fields) => validate_only_given_fields(fields, schema)?,
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
    plain_fields: Option<&PlainFields>,
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
            previous_operations: Some(previous_operations),
            fields: None,
        }),
        None => Err(ValidateOperationError::ExpectedPreviousOperations),
    }
}
