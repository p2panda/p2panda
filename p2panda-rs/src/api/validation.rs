// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::Human;
use crate::api::ValidationError;
use crate::document::DocumentId;
use crate::hash::{Hash, HashId};
use crate::operation::OperationAction;
use crate::operation::body::Body;
use crate::operation::body::plain::PlainOperation;
use crate::operation::body::traits::Schematic;
use crate::operation::error::ValidateOperationError;
use crate::operation::header::{Header, HeaderExtension, HeaderAction};
use crate::operation::traits::AsOperation;
use crate::schema::validate::{validate_all_fields, validate_only_given_fields};
use crate::schema::{SchemaId, Schema};


pub fn validate_header_extensions(header: &Header) -> Result<(), ValidateOperationError> {
    let HeaderExtension {
        action,
        document_id,
        previous,
        timestamp,
        backlink,
        depth,
        ..
    } = &header.4;

    // All operations require a timestamp
    if timestamp.is_none() {
        return Err(ValidateOperationError::ExpectedTimestamp);
    }

    // All operations require a depth
    let depth = match depth {
        Some(depth) => depth,
        None => return Err(ValidateOperationError::ExpectedDepth),
    };

    match (action, document_id) {
        // Operations with no action set in their header and without a document id are CREATE operations.
        (None, None) => {
            if backlink.is_some() {
                return Err(ValidateOperationError::UnexpectedBacklink);
            }

            if previous.is_some() {
                return Err(ValidateOperationError::UnexpectedPreviousOperations);
            }

            if *depth != 0 {
                return Err(ValidateOperationError::ExpectedZeroDepth);
            }
            Ok(())
        }
        // Operations with the document id set are either UPDATE or DELETE operations.
        (_, Some(_)) => {
            if backlink.is_none() {
                return Err(ValidateOperationError::ExpectedBacklink);
            }

            if previous.is_none() {
                return Err(ValidateOperationError::ExpectedPreviousOperations);
            }

            if *depth == 0 {
                return Err(ValidateOperationError::ExpectedNonZeroDepth);
            }
            Ok(())
        }
        // If the DELETE header action is set then we expect a document id as well.
        (Some(HeaderAction::Delete), None) => {
            return Err(ValidateOperationError::ExpectedDocumentId)
        }
    }
}

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
        (OperationAction::Create, Some(fields)) => {
            validate_all_fields(&fields, schema).map(|fields| Some(fields))
        }
        (OperationAction::Update, Some(fields)) => {
            validate_only_given_fields(&fields, schema).map(|fields| Some(fields))
        }
        (OperationAction::Delete, None) => Ok(None),
        (OperationAction::Delete, Some(_)) => return Err(ValidateOperationError::UnexpectedFields),
        (OperationAction::Create | OperationAction::Update, None) => {
            return Err(ValidateOperationError::ExpectedFields)
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

    if operation.timestamp() <= previous_timestamp {
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

    if operation.timestamp() <= backlink_timestamp {
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
