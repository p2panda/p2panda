// SPDX-License-Identifier: AGPL-3.0-or-later

//! Collection of low-level validation methods for operations.
use crate::operation::body::plain::PlainOperation;
use crate::operation::body::traits::Schematic;
use crate::operation::body::Body;
use crate::operation::error::ValidateOperationError;
use crate::operation::OperationAction;
use crate::schema::validate::{validate_all_fields, validate_only_given_fields};
use crate::schema::Schema;
use crate::Human;

/// Checks the fields and format of an operation against a schema.
pub fn validate_plain_operation(
    action: &OperationAction,
    plain_operation: &PlainOperation,
    schema: &Schema,
) -> Result<Body, ValidateOperationError> {
    let claimed_schema_id = plain_operation.schema_id();

    // Make sure the schema id and given schema matches
    if claimed_schema_id != schema.id() {
        return Err(ValidateOperationError::SchemaNotMatching(
            claimed_schema_id.display(),
            schema.id().display(),
        ));
    }

    let fields = match (action, plain_operation.plain_fields()) {
        (OperationAction::Create, Some(fields)) => validate_all_fields(&fields, schema).ok(),
        (OperationAction::Update, Some(fields)) => validate_only_given_fields(&fields, schema).ok(),
        (OperationAction::Delete, None) => None,
        // All other cases should not occur if correct validation of operation has been performed.
        (_, _) => unreachable!(),
    };

    Ok(Body(schema.id().clone(), fields))
}
