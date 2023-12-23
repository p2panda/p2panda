// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::api::ValidationError;
use crate::hash::{Hash, HashId};
use crate::operation::traits::AsOperation;

pub fn validate_previous(
    operation: &impl AsOperation,
    previous: &Vec<impl AsOperation>,
) -> Result<(), ValidationError> {
    for previous_operation in previous {
        if operation.schema_id() != previous_operation.schema_id() {
            return Err(ValidationError::MismathingSchemaInPrevious(
                previous_operation.id().clone(),
                previous_operation.schema_id().clone(),
                operation.schema_id().clone(),
            )
            .into());
        }

        if operation.document_id() != previous_operation.document_id() {
            return Err(ValidationError::MismathingDocumentIdInPrevious(
                previous_operation.id().clone(),
                previous_operation.document_id(),
                operation.document_id(),
            )
            .into());
        }

        if operation.depth() <= previous_operation.depth() {
            return Err(ValidationError::DepthLessThanPrevious(
                operation.id().clone(),
                operation.depth(),
            )
            .into());
        }

        if operation.timestamp() <= previous_operation.timestamp() {
            return Err(ValidationError::TimestampLessThanPrevious(
                operation.id().clone(),
                operation.timestamp(),
            )
            .into());
        }
    }
    Ok(())
}

pub fn validate_backlink(
    operation: &impl AsOperation,
    claimed_backlink: &Hash,
    latest_operation: &impl AsOperation,
) -> Result<(), ValidationError> {
    if claimed_backlink != latest_operation.id().as_hash() {
        return Err(ValidationError::IncorrectBacklink(
            operation.id().as_hash().clone(),
            operation.public_key().clone(),
            operation.document_id(),
            latest_operation.id().as_hash().clone(),
        )
        .into());
    }

    if operation.timestamp() <= latest_operation.timestamp() {
        return Err(ValidationError::TimestampLessThanBacklink(
            operation.id().clone(),
            operation.timestamp(),
        )
        .into());
    }

    if operation.depth() <= latest_operation.depth() {
        return Err(ValidationError::DepthLessThanBacklink(
            operation.id().clone(),
            operation.depth(),
        )
        .into());
    }
    Ok(())
}
