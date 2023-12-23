// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::api::ValidationError;
use crate::document::DocumentId;
use crate::hash::{Hash, HashId};
use crate::operation::traits::AsOperation;
use crate::schema::SchemaId;

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
