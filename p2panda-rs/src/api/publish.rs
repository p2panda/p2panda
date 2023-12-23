// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::api::{DomainError, ValidationError};
use crate::document::DocumentViewId;
use crate::hash::HashId;
use crate::operation::body::plain::PlainOperation;
use crate::operation::body::traits::Schematic;
use crate::operation::body::EncodedBody;
use crate::operation::header::decode::decode_header;
use crate::operation::header::traits::{Actionable, Authored};
use crate::operation::header::validate::{validate_payload, verify_signature};
use crate::operation::header::EncodedHeader;
use crate::operation::traits::AsOperation;
use crate::operation::validate::validate_plain_operation;
use crate::operation::Operation;
use crate::schema::Schema;
use crate::storage_provider::traits::OperationStore;

use crate::api::{validate_backlink, validate_previous};

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

    // Verify the operations' signature against it's public key.
    verify_signature(header.public_key(), &header.signature(), encoded_header)?;

    // Validate the plain fields against claimed schema and produce an operation Body.
    let body = validate_plain_operation(&header.action(), &plain_operation, schema)?;

    // Construct the operation. This performs internal validation to check the header and body
    // combine into a valid p2panda operation.
    let operation = Operation::new(encoded_header.hash().into(), header, body)?;

    let latest_operation = store
        .get_latest_operation(&operation.document_id(), operation.public_key())
        .await?;

    // Validate the authors document log integrity:
    // - if a backlink is given it should point to the latest operation for this document and
    //   public key, and the new operation should have a greater timestamp and depth.
    // - if no backlink is given no log should exist for this document and public key
    match (operation.backlink(), latest_operation) {
        (None, None) => Ok(()),
        (None, Some(_)) => Err(ValidationError::UnexpectedDocumentLog(
            operation.public_key().clone(),
            operation.document_id(),
        )),
        (Some(_), None) => Err(ValidationError::ExpectedDocumentLog(
            operation.public_key().clone(),
            operation.document_id(),
        )),
        (Some(backlink), Some(latest_operation)) => {
            validate_backlink(&operation, &backlink, &latest_operation)
        }
    }?;

    // Validate the operations contained in `previous``:
    // - all schema id should match the schema id of the new operation
    // - all timestamps should be lower than the new operation's timestamp
    // - all depths should be lower than the new operation's depth
    // - all document ids should match the document id of the new operation
    if let Some(previous) = operation.previous() {
        // Get all operations contained in this operations previous.
        let previous_operations = get_view_id_operations(store, previous).await?;
        validate_previous(&operation, &previous_operations)?;
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

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use crate::api::publish;
    use crate::identity::KeyPair;
    use crate::operation::body::encode::encode_body;
    use crate::operation::header::encode::encode_header;
    use crate::operation::OperationBuilder;
    use crate::schema::Schema;
    use crate::test_utils::constants::test_fields;
    use crate::test_utils::fixtures::{key_pair, schema};
    use crate::test_utils::memory_store::MemoryStore;

    #[rstest]
    #[tokio::test]
    async fn operation_builder_create(key_pair: KeyPair, schema: Schema) {
        let store = MemoryStore::default();

        let operation = OperationBuilder::new(schema.id())
            .fields(&test_fields())
            .sign(&key_pair)
            .unwrap();

        let encoded_header = encode_header(operation.header()).unwrap();
        let encoded_body = encode_body(operation.body()).unwrap();

        assert!(publish(
            &store,
            &schema,
            &encoded_header,
            &operation.body().into(),
            &encoded_body,
        )
        .await
        .is_ok());
    }
}
