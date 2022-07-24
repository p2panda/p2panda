// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::document::DocumentViewId;
use crate::operation::{
    Operation, OperationAction, OperationFields, OperationVersion, RawFields, RawOperation,
    RawOperationError,
};
use crate::schema::{verify_all_fields, verify_only_given_fields, Schema};

pub fn verify_schema_and_convert(
    raw_operation: &RawOperation,
    schema: &Schema,
) -> Result<Operation, RawOperationError> {
    if raw_operation.version() != OperationVersion::V1 {
        // @TODO: This will be handled during deserialization
    }

    match raw_operation.action() {
        OperationAction::Create => verify_create_operation(
            raw_operation.previous_operations(),
            raw_operation.fields(),
            &schema,
        ),
        OperationAction::Update => verify_update_operation(
            raw_operation.previous_operations(),
            raw_operation.fields(),
            &schema,
        ),
        OperationAction::Delete => verify_delete_operation(
            raw_operation.previous_operations(),
            raw_operation.fields(),
            &schema,
        ),
    }
}

fn verify_create_operation(
    raw_previous_operations: Option<&DocumentViewId>,
    raw_fields: Option<&RawFields>,
    schema: &Schema,
) -> Result<Operation, RawOperationError> {
    if raw_previous_operations.is_some() {
        return Err(RawOperationError::UnexpectedPreviousOperations);
    }

    let validated_fields = match raw_fields {
        Some(fields) => verify_all_fields(&fields, &schema)?,
        None => return Err(RawOperationError::ExpectedFields),
    };

    // Unwrap here as we already should have done all validation before
    Ok(Operation::new_create(schema.id().to_owned(), validated_fields).unwrap())
}

fn verify_update_operation(
    raw_previous_operations: Option<&DocumentViewId>,
    raw_fields: Option<&RawFields>,
    schema: &Schema,
) -> Result<Operation, RawOperationError> {
    let mut validated_fields = OperationFields::new();

    let validated_fields = match raw_fields {
        Some(fields) => verify_only_given_fields(&fields, &schema)?,
        None => return Err(RawOperationError::ExpectedFields),
    };

    match raw_previous_operations {
        Some(previous_operations) => {
            Ok(Operation::new_update(
                schema.id().to_owned(),
                previous_operations.to_owned(),
                validated_fields,
            )
            // Unwrap here as we already should have done all validation before
            .unwrap())
        }
        None => Err(RawOperationError::ExpectedPreviousOperations),
    }
}

fn verify_delete_operation(
    raw_previous_operations: Option<&DocumentViewId>,
    raw_fields: Option<&RawFields>,
    schema: &Schema,
) -> Result<Operation, RawOperationError> {
    if raw_fields.is_some() {
        return Err(RawOperationError::UnexpectedFields);
    }

    match raw_previous_operations {
        Some(previous_operations) => {
            Ok(
                Operation::new_delete(schema.id().to_owned(), previous_operations.to_owned())
                    // Unwrap here as we already should have done all validation before
                    .unwrap(),
            )
        }
        None => {
            return Err(RawOperationError::ExpectedPreviousOperations);
        }
    }
}
