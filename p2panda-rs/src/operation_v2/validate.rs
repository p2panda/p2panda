// SPDX-License-Identifier: AGPL-3.0-or-later

//! Collection of low-level validation methods for operations.
use crate::operation_v2::body::traits::Schematic;
use crate::operation_v2::error::ValidateOperationError;
use crate::operation_v2::header::traits::Actionable;
use crate::operation_v2::{Operation, OperationAction};
use crate::schema::validate::{validate_all_fields, validate_only_given_fields};
use crate::schema::Schema;
use crate::Human;

/// Checks the fields and format of an operation against a schema.
pub fn validate_operation(
    operation: &Operation,
    schema: &Schema,
) -> Result<(), ValidateOperationError> {
    let claimed_schema_id = operation.schema_id();

    // Make sure the schema id and given schema matches
    if claimed_schema_id != schema.id() {
        return Err(ValidateOperationError::SchemaNotMatching(
            claimed_schema_id.display(),
            schema.id().display(),
        ));
    }

    let _ = match (operation.action(), operation.plain_fields()) {
        (OperationAction::Create, Some(fields)) => validate_all_fields(&fields, schema),
        (OperationAction::Update, Some(fields)) => validate_only_given_fields(&fields, schema),
        (OperationAction::Delete, None) => return Ok(()),
        // All other cases should not occur if correct validation of operation has been performed.
        (_, _) => unreachable!(),
    }?;

    Ok(())
}
