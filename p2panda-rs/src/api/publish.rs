// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::api::{DomainError, ValidationError};
use crate::document::DocumentViewId;
use crate::operation::body::plain::PlainOperation;
use crate::operation::body::traits::Schematic;
use crate::operation::body::EncodedBody;
use crate::operation::header::decode::decode_header;
use crate::operation::header::traits::Actionable;
use crate::operation::header::validate::validate_payload;
use crate::operation::header::EncodedHeader;
use crate::operation::traits::AsOperation;
use crate::operation::validate::validate_plain_operation;
use crate::operation::Operation;
use crate::schema::Schema;
use crate::storage_provider::traits::OperationStore;

pub async fn publish<S: OperationStore>(
    store: &S,
    schema: &Schema,
    encoded_header: &EncodedHeader,
    plain_operation: &PlainOperation,
    encoded_body: &EncodedBody,
) -> Result<(), DomainError> {
    // Decode the header.
    let header = decode_header(encoded_header)?;

    // Validate the payload.
    validate_payload(&header, encoded_body)?;

    // Validate the plain fields against claimed schema and produce an operation Body.
    let body = validate_plain_operation(&header.action(), &plain_operation, schema)?;

    // Construct the operation. This performs internal validation to check the header and body
    // combine into a valid p2panda operation.
    let operation = Operation::new(encoded_header.hash().into(), header, body)?;

    // @TODO: Check that the backlink exists and no fork has occurred.

    if let Some(previous) = operation.previous() {
        // Get all operations contained in this operations previous.
        let mut previous_operations = get_view_id_operations(store, previous).await?;

        // Check that all schema ids are the same.
        let all_previous_have_same_schema_id = previous_operations
            .iter()
            .all(|previous_operation| previous_operation.schema_id() == operation.schema_id());

        if !all_previous_have_same_schema_id {
            return Err(ValidationError::InvalidClaimedSchema(
                operation.id().clone(),
                operation.schema_id().clone(),
            )
            .into());
        };

        // Check that all timestamps are lower.
        let all_previous_timestamps_are_lower = previous_operations
            .iter()
            .all(|previous_operation| previous_operation.timestamp() < operation.timestamp());

        if !all_previous_timestamps_are_lower {
            return Err(ValidationError::InvalidTimestamp(
                operation.id().clone(),
                operation.timestamp(),
            )
            .into());
        };

        // Check that all operations in previous originate from the same document.
        previous_operations.dedup_by(|a, b| a.document_id() == b.document_id());
        if previous_operations.len() > 1 {
            return Err(ValidationError::InvalidDocumentViewId.into());
        }

        // Check that the document id of all previous operations match the published operation.
        //
        // We can unwrap here as we know there is one operation id in previous_operations.
        if previous_operations.first().unwrap().document_id() != operation.document_id() {
            return Err(ValidationError::IncorrectDocumentId(
                operation.id().clone(),
                operation.document_id(),
            )
            .into());
        }
    }

    // Insert the operation into the store.
    store.insert_operation(&operation).await?;
    Ok(())
}

pub async fn get_view_id_operations<S: OperationStore>(
    store: &S,
    view_id: &DocumentViewId,
) -> Result<Vec<impl AsOperation>, ValidationError> {
    let mut found_operations = vec![];
    for id in view_id.iter() {
        let operation = store.get_operation(id).await?;
        if let Some(operation) = operation {
            found_operations.push(operation)
        } else {
            return Err(ValidationError::PreviousOperationNotFound(id.clone()));
        }
    }
    Ok(found_operations)
}
