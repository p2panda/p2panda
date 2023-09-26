// SPDX-License-Identifier: AGPL-3.0-or-later

//! Collection of low-level validation methods for operations.
use crate::document::DocumentViewId;
use crate::operation_v2::body::plain::{PlainFields, PlainOperation};
use crate::operation_v2::body::traits::Schematic;
use crate::operation_v2::body::EncodedBody;
use crate::operation_v2::error::ValidateOperationError;
use crate::operation_v2::header::traits::Actionable;
use crate::operation_v2::header::validate::validate_payload;
use crate::operation_v2::header::{EncodedHeader, Header};
use crate::operation_v2::{Operation, OperationAction, OperationId, OperationVersion};
use crate::schema::validate::{validate_all_fields, validate_only_given_fields};
use crate::schema::Schema;
use crate::Human;

#[allow(clippy::too_many_arguments)]
pub fn validate_operation_with_header(
    header: &Header,
    encoded_header: &EncodedHeader,
    plain_operation: &PlainOperation,
    encoded_body: &EncodedBody,
    schema: &Schema,
) -> Result<(Operation, OperationId), ValidateOperationError> {
    // Verify that the entry belongs to this operation
    validate_payload(header, encoded_body)?;

    // The operation id is the result of a hashing function over the entry bytes.
    let operation_id = encoded_header.hash().into();

    // Validate and convert plain operation with the help of a schema
    let operation = validate_operation(plain_operation, schema)?;

    Ok((operation, operation_id))
}

/// Check the format of an operation-like data type.
///
/// This method checks against:
///
/// 1. Correct operation format (#OP2)
pub fn validate_operation_format(
    operation: &(impl Actionable + Schematic),
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
pub fn validate_operation(
    operation: &(impl Actionable + Schematic),
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
