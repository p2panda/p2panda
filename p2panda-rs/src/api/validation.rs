// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::api::ValidationError;
use crate::document::DocumentId;
use crate::hash::{Hash, HashId};
use crate::operation::body::plain::PlainOperation;
use crate::operation::body::traits::Schematic;
use crate::operation::body::Body;
use crate::operation::header::traits::Actionable;
use crate::operation::header::{Header, HeaderExtension};
use crate::operation::traits::AsOperation;
use crate::operation::OperationAction;
use crate::schema::validate::{validate_all_fields, validate_only_given_fields};
use crate::schema::{Schema, SchemaId};
use crate::{Human, Validate};

use super::error::ValidatePlainOperationError;

/// Checks the fields and format of an operation against a schema.
pub fn validate_plain_operation(
    action: &OperationAction,
    plain_operation: &PlainOperation,
    schema: &Schema,
) -> Result<Body, ValidatePlainOperationError> {
    let claimed_schema_id = plain_operation.schema_id();

    // Make sure the schema id and given schema matches
    if claimed_schema_id != schema.id() {
        return Err(ValidatePlainOperationError::SchemaNotMatching(
            claimed_schema_id.display(),
            schema.id().display(),
        ));
    }

    let fields = match (action, plain_operation.plain_fields()) {
        (OperationAction::Create, Some(fields)) => {
            validate_all_fields(&fields, schema).map(|fields| Some(fields))
        }
        (OperationAction::Update, Some(fields)) => {
            validate_only_given_fields(&fields, schema).map(|fields| Some(fields))
        }
        (OperationAction::Delete, None) => Ok(None),
        (OperationAction::Delete, Some(_)) => {
            return Err(ValidatePlainOperationError::UnexpectedFields)
        }
        (OperationAction::Create | OperationAction::Update, None) => {
            return Err(ValidatePlainOperationError::ExpectedFields)
        }
    }?;

    Ok(Body(schema.id().clone(), fields))
}

pub fn validate_previous(
    operation: &impl AsOperation,
    previous_schema_id: &SchemaId,
    previous_document_id: &DocumentId,
    previous_depth: u64,
    previous_timestamp: u128,
) -> Result<(), ValidationError> {
    if operation.schema_id() != previous_schema_id {
        return Err(ValidationError::MismathingSchemaInPrevious(
            operation.id().clone(),
            previous_schema_id.clone(),
            operation.schema_id().clone(),
        )
        .into());
    }

    if operation.document_id() != previous_document_id.clone() {
        return Err(ValidationError::MismathingDocumentIdInPrevious(
            operation.id().clone(),
            previous_document_id.clone(),
            operation.document_id(),
        )
        .into());
    }

    if operation.depth() <= previous_depth {
        return Err(ValidationError::DepthLessThanPrevious(
            operation.id().clone(),
            operation.depth(),
        )
        .into());
    }

    // timestamp can be equal to previous timestamp.
    if operation.timestamp() < previous_timestamp {
        return Err(ValidationError::TimestampLessThanPrevious(
            operation.id().clone(),
            operation.timestamp(),
        )
        .into());
    }
    Ok(())
}

pub fn validate_backlink(
    operation: &impl AsOperation,
    claimed_backlink: &Hash,
    backlink_hash: &Hash,
    backlink_depth: u64,
    backlink_timestamp: u128,
) -> Result<(), ValidationError> {
    if claimed_backlink != backlink_hash {
        return Err(ValidationError::IncorrectBacklink(
            operation.id().as_hash().clone(),
            operation.public_key().clone(),
            operation.document_id(),
            backlink_hash.clone(),
        )
        .into());
    }

    if operation.timestamp() < backlink_timestamp {
        return Err(ValidationError::TimestampLessThanBacklink(
            operation.id().clone(),
            operation.timestamp(),
        )
        .into());
    }

    if operation.depth() <= backlink_depth {
        return Err(ValidationError::DepthLessThanBacklink(
            operation.id().clone(),
            operation.depth(),
        )
        .into());
    }
    Ok(())
}
