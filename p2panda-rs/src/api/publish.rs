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
        let previous_operations = get_view_id_operations(store, previous).await?;

        // Check that all document ids are the same.
        let all_previous_from_same_document = previous_operations
            .iter()
            .all(|previous_operation| previous_operation.document_id() == operation.document_id());

        if !all_previous_from_same_document {
            return Err(ValidationError::IncorrectDocumentId(
                operation.id().clone(),
                operation.document_id(),
            )
            .into());
        }

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

//
// #[cfg(test)]
// mod tests {
//     use rstest::rstest;
//
//     use crate::api::{next_args, publish};
//     use crate::document::{DocumentId, DocumentViewId};
//     use crate::entry::encode::sign_and_encode_entry;
//     use crate::entry::traits::{AsEncodedEntry, AsEntry};
//     use crate::entry::{LogId, SeqNum};
//     use crate::hash::Hash;
//     use crate::identity::KeyPair;
//     use crate::operation::decode::decode_operation;
//     use crate::operation::encode::encode_operation;
//     use crate::operation::{
//         Operation, OperationAction, OperationBuilder, OperationId, OperationValue,
//     };
//     use crate::schema::{FieldType, Schema, SchemaId, SchemaName};
//     use crate::storage_provider::traits::{EntryStore, LogStore, OperationStore};
//     use crate::test_utils::constants::{test_fields, PRIVATE_KEY};
//     use crate::test_utils::fixtures::{
//         create_operation, delete_operation, key_pair, operation, populate_store_config,
//         random_document_view_id, random_hash, random_operation_id, schema, update_operation,
//     };
//     use crate::test_utils::memory_store::helpers::{
//         populate_store, remove_entries, remove_operations, send_to_store, PopulateStoreConfig,
//     };
//     use crate::test_utils::memory_store::{MemoryStore, StorageEntry};
//     use crate::WithId;
//
//     use super::{determine_document_id, validate_entry_and_operation};
//
//     type LogIdAndSeqNum = (u64, u64);
//
//     #[rstest]
//     #[tokio::test]
//     async fn determines_document_id(
//         random_operation_id: OperationId,
//         #[with(test_fields(), random_document_view_id())] update_operation: Operation,
//         #[from(populate_store_config)]
//         #[with(4, 1, 1)]
//         config: PopulateStoreConfig,
//     ) {
//         let store = MemoryStore::default();
//         let (key_pairs, document_ids) = populate_store(&store, &config).await;
//         let document_id = document_ids.get(0).unwrap();
//         let public_key = key_pairs[0].public_key();
//
//         // Get first entry and operation.
//         let entry_one = store
//             .get_entry_at_seq_num(&public_key, &LogId::new(0), &SeqNum::new(1).unwrap())
//             .await
//             .unwrap()
//             .unwrap();
//
//         let operation_one = store
//             .get_operation(&entry_one.hash().into())
//             .await
//             .unwrap()
//             .unwrap();
//
//         // Get forth entry and operation.
//         let entry_four = store
//             .get_entry_at_seq_num(&public_key, &LogId::new(0), &SeqNum::new(4).unwrap())
//             .await
//             .unwrap()
//             .unwrap();
//
//         let operation_four = store
//             .get_operation(&entry_four.hash().into())
//             .await
//             .unwrap()
//             .unwrap();
//
//         // Use the first operation to determine document id.
//         let id = determine_document_id(&store, &operation_one, operation_one.id())
//             .await
//             .unwrap();
//         assert_eq!(document_id, &id);
//
//         // Use the forth operation to determine document id.
//         let id = determine_document_id(&store, &operation_four, operation_four.id())
//             .await
//             .unwrap();
//         assert_eq!(document_id, &id);
//
//         // Use a random operation to determine document id (should error).
//         let result = determine_document_id(&store, &update_operation, &random_operation_id).await;
//         assert!(result.is_err())
//     }
//
//     #[rstest]
//     #[tokio::test]
//     async fn determines_document_id_deleted_document(
//         #[from(populate_store_config)]
//         #[with(4, 1, 1, true)]
//         config: PopulateStoreConfig,
//     ) {
//         let store = MemoryStore::default();
//         let (key_pairs, _) = populate_store(&store, &config).await;
//         let public_key = key_pairs[0].public_key();
//
//         // Get first entry and operation.
//         let entry_one = store
//             .get_entry_at_seq_num(&public_key, &LogId::new(0), &SeqNum::new(1).unwrap())
//             .await
//             .unwrap()
//             .unwrap();
//
//         let operation_one = store
//             .get_operation(&entry_one.hash().into())
//             .await
//             .unwrap()
//             .unwrap();
//
//         // Get forth entry and operation.
//         let entry_four = store
//             .get_entry_at_seq_num(&public_key, &LogId::new(0), &SeqNum::new(4).unwrap())
//             .await
//             .unwrap()
//             .unwrap();
//
//         let operation_four = store
//             .get_operation(&entry_four.hash().into())
//             .await
//             .unwrap()
//             .unwrap();
//
//         // Use the first operation to determine document id, should error as this document is deleted.
//         let result = determine_document_id(&store, &operation_one, operation_one.id()).await;
//         assert!(result.is_err());
//
//         // Use the forth operation to determine document id, should error as this document is deleted.
//         let result = determine_document_id(&store, &operation_four, operation_four.id()).await;
//         assert!(result.is_err());
//     }
//
//     #[rstest]
//     #[case::ok(&[(0, 8)], (0, 8))]
//     #[should_panic(
//         expected = "Expected skiplink entry not found in store: public key 2f8e50c2ede6d936ecc3144187ff1c273808185cfbc5ff3d3748d1ff7353fc96, log id 0, seq num 4"
//     )]
//     #[case::skiplink_missing(&[(0, 4), (0, 8)], (0, 8))]
//     #[should_panic(
//         expected = "Entry's claimed seq num of 8 does not match expected seq num of 7 for given public key and log"
//     )]
//     #[case::backlink_missing(&[(0, 7), (0, 8)], (0, 8))]
//     #[should_panic(
//         expected = "Entry's claimed seq num of 8 does not match expected seq num of 7 for given public key and log"
//     )]
//     #[case::backlink_and_skiplink_missing(&[(0, 4), (0, 7), (0, 8)], (0, 8))]
//     #[should_panic(
//         expected = "Entry's claimed seq num of 8 does not match expected seq num of 9 for given public key and log"
//     )]
//     #[case::seq_num_occupied_again(&[], (0, 8))]
//     #[should_panic(
//         expected = "Entry's claimed seq num of 7 does not match expected seq num of 9 for given public key and log"
//     )]
//     #[case::seq_num_occupied_(&[], (0, 7))]
//     #[should_panic(
//         expected = "Entry's claimed seq num of 8 does not match expected seq num of 1 for given public key and log"
//     )]
//     #[case::no_entries_yet(&[(0, 1), (0, 2), (0, 3), (0, 4), (0, 5), (0, 6), (0, 7), (0, 8)], (0, 8))]
//     #[tokio::test]
//     async fn validate_against_entries_in_store(
//         schema: Schema,
//         #[case] entries_to_remove: &[LogIdAndSeqNum],
//         #[case] entry_to_publish: LogIdAndSeqNum,
//         #[from(populate_store_config)]
//         #[with(8, 1, 1)]
//         config: PopulateStoreConfig,
//     ) {
//         let store = MemoryStore::default();
//         let (key_pairs, _) = populate_store(&store, &config).await;
//
//         // The public key who has published to the db.
//         let public_key = key_pairs[0].public_key();
//
//         // Get the latest entry from the db.
//         let entry = store
//             .get_entry_at_seq_num(
//                 &public_key,
//                 &LogId::new(entry_to_publish.0),
//                 &SeqNum::new(entry_to_publish.1).unwrap(),
//             )
//             .await
//             .unwrap()
//             .unwrap();
//
//         // Remove some entries and their operations from the database.
//         remove_operations(&store, &public_key, entries_to_remove);
//         remove_entries(&store, &public_key, entries_to_remove);
//
//         // Validate the entry and operation.
//         let operation = entry.payload().unwrap();
//         let plain_operation = decode_operation(operation).unwrap();
//         validate_entry_and_operation(&store, &schema, &entry, &entry, &plain_operation, operation)
//             // Unwrap here causing a panic, we check the errors match what we expect.
//             .await
//             .map_err(|err| err.to_string())
//             .unwrap();
//     }
//
//     #[rstest]
//     #[should_panic(
//         expected = "Expected skiplink entry not found in store: public key 2f8e50c2ede6d936ecc3144187ff1c273808185cfbc5ff3d3748d1ff7353fc96, log id 0, seq num 4"
//     )]
//     #[case::next_args_skiplink_missing(&[(0, 4), (0, 7), (0, 8)], (0, 7))]
//     #[tokio::test]
//     async fn next_args_skiplink_missing(
//         schema: Schema,
//         #[case] entries_to_remove: &[LogIdAndSeqNum],
//         #[case] entry_to_publish: LogIdAndSeqNum,
//         #[from(populate_store_config)]
//         #[with(8, 1, 1)]
//         config: PopulateStoreConfig,
//     ) {
//         let store = MemoryStore::default();
//         let (key_pairs, _) = populate_store(&store, &config).await;
//
//         // The public key who has published to the db.
//         let public_key = key_pairs[0].public_key();
//
//         // Get the latest entry from the db.
//         let entry = store
//             .get_entry_at_seq_num(
//                 &public_key,
//                 &LogId::new(entry_to_publish.0),
//                 &SeqNum::new(entry_to_publish.1).unwrap(),
//             )
//             .await
//             .unwrap()
//             .unwrap();
//
//         // Remove some entries and their operations from the database.
//         remove_operations(&store, &public_key, entries_to_remove);
//         remove_entries(&store, &public_key, entries_to_remove);
//
//         // Publish the entry and see what happens.
//         let operation = entry.payload.unwrap();
//         publish(
//             &store,
//             &schema,
//             &entry.encoded_entry,
//             &decode_operation(&operation).unwrap(),
//             &operation,
//         )
//         .await
//         .map_err(|err| err.to_string())
//         .unwrap();
//     }
//
//     #[rstest]
//     #[case::ok_single_writer(
//         &[],
//         &[(0, 8)],
//         KeyPair::from_private_key_str(PRIVATE_KEY).unwrap()
//     )]
//     // Weird case where all previous operations are on the same branch, but still valid.
//     #[case::ok_many_previous(
//         &[],
//         &[(0, 8), (0, 7), (0, 6)],
//         KeyPair::from_private_key_str(PRIVATE_KEY).unwrap()
//     )]
//     #[case::ok_multi_writer(
//         &[],
//         &[(0, 8)],
//         KeyPair::new()
//     )]
//     #[should_panic(
//         expected = "Previous operation 00202df2f7c15280a319f42f1b2df51cd8dcaa79286428ff48301309d3bb37868981 not found in store"
//     )]
//     #[case::previous_operation_missing(
//         &[(0, 8)],
//         &[(0, 8)],
//         KeyPair::from_private_key_str(PRIVATE_KEY).unwrap()
//     )]
//     #[should_panic(
//         expected = "Previous operation 0020397d5f246d6124d1aa6fb5fcdb2a0f202bafe0aecb6ff1423fa2164ae4403204 not found in store"
//     )]
//     #[case::one_of_some_previous_missing(
//         &[(0, 7)],
//         &[(0, 7), (0, 8)],
//         KeyPair::from_private_key_str(PRIVATE_KEY).unwrap()
//     )]
//     #[should_panic(
//         expected = "Previous operation 00202df2f7c15280a319f42f1b2df51cd8dcaa79286428ff48301309d3bb37868981 not found in store"
//     )]
//     #[case::one_of_some_previous_missing(
//         &[(0, 8)],
//         &[(0, 7), (0, 8)],
//         KeyPair::from_private_key_str(PRIVATE_KEY).unwrap()
//     )]
//     #[should_panic(
//         expected = "Previous operation 00202df2f7c15280a319f42f1b2df51cd8dcaa79286428ff48301309d3bb37868981 not found in store"
//     )]
//     #[case::missing_previous_operation_multi_writer(
//         &[(0, 8)],
//         &[(0, 8)],
//         KeyPair::new()
//     )]
//     #[should_panic(
//         expected = "Operations in passed document view id originate from different documents"
//     )]
//     #[case::previous_invalid_multiple_document_id(
//         &[],
//         &[(0, 8), (1, 8)],
//         KeyPair::from_private_key_str(PRIVATE_KEY).unwrap()
//     )]
//     #[tokio::test]
//     async fn validates_against_operations_in_store(
//         schema: Schema,
//         // The operations to be removed from the db
//         #[case] operations_to_remove: &[LogIdAndSeqNum],
//         // The previous operations described by their log id and seq number (log_id, seq_num)
//         #[case] previous: &[LogIdAndSeqNum],
//         #[case] key_pair: KeyPair,
//         #[from(populate_store_config)]
//         #[with(8, 2, 1)]
//         config: PopulateStoreConfig,
//     ) {
//         let store = MemoryStore::default();
//         let (key_pairs, documents) = populate_store(&store, &config).await;
//
//         let existing_author = key_pairs[0].public_key();
//
//         // Get the document id.
//         let document = documents.first().map(|id| id.as_str().parse().unwrap());
//
//         // Map the passed &[LogIdAndSeqNum] into a DocumentViewId containing the claimed operations.
//         let previous: Vec<OperationId> = previous
//             .iter()
//             .filter_map(|(log_id, seq_num)| {
//                 store
//                     .entries
//                     .lock()
//                     .unwrap()
//                     .values()
//                     .find(|entry| {
//                         entry.seq_num().as_u64() == *seq_num
//                             && entry.log_id().as_u64() == *log_id
//                             && *entry.public_key() == existing_author
//                     })
//                     .map(|entry| entry.hash().into())
//             })
//             .collect();
//
//         // Construct document view id for previous operations.
//         let document_view_id = DocumentViewId::new(&previous);
//
//         // Compose the next operation.
//         let operation = OperationBuilder::new(schema.id())
//             .action(OperationAction::Update)
//             .previous(&document_view_id)
//             .fields(&test_fields())
//             .build()
//             .unwrap();
//
//         // The next args for a author who will publish the next entry based on
//         // the passed key pair for this test run.
//         let (backlink, skiplink, seq_num, log_id) =
//             next_args(&store, &key_pair.public_key(), document.as_ref())
//                 .await
//                 .unwrap();
//
//         let encoded_operation = encode_operation(&operation).unwrap();
//         let encoded_entry = sign_and_encode_entry(
//             &log_id,
//             &seq_num,
//             skiplink.map(Hash::from).as_ref(),
//             backlink.map(Hash::from).as_ref(),
//             &encoded_operation,
//             &key_pair,
//         )
//         .unwrap();
//
//         // Remove some operations from the db.
//         remove_operations(&store, &existing_author, operations_to_remove);
//
//         // Publish the entry and operation.
//         let result = publish(
//             &store,
//             &schema,
//             &encoded_entry,
//             &decode_operation(&encoded_operation).unwrap(),
//             &encoded_operation,
//         )
//         .await;
//
//         // Unwrap here causing a panic, we check the errors match what we expect.
//         result.map_err(|err| err.to_string()).unwrap();
//     }
//
//     #[rstest]
//     #[case::owner_publishes_update_to_correct_log(
//         LogId::new(0),
//         KeyPair::from_private_key_str(PRIVATE_KEY).unwrap())
//     ]
//     #[case::new_author_updates_to_new_log(LogId::new(0), KeyPair::new())]
//     #[should_panic(
//         expected = "Entry's claimed log id of 1 does not match existing log id of 0 for given public key and document"
//     )]
//     #[case::owner_updates_to_wrong_and_taken_log(LogId::new(1), KeyPair::from_private_key_str(PRIVATE_KEY).unwrap())]
//     #[should_panic(
//         expected = "Entry's claimed log id of 2 does not match existing log id of 0 for given public key and document"
//     )]
//     #[case::owner_updates_to_wrong_but_free_log(LogId::new(2), KeyPair::from_private_key_str(PRIVATE_KEY).unwrap())]
//     #[tokio::test]
//     async fn new_author_updates_existing_document(
//         schema: Schema,
//         #[case] log_id: LogId,
//         #[case] key_pair: KeyPair,
//         #[from(populate_store_config)]
//         #[with(2, 1, 1)]
//         config: PopulateStoreConfig,
//     ) {
//         let store = MemoryStore::default();
//         let (_, documents) = populate_store(&store, &config).await;
//
//         let document_id = documents.first().unwrap();
//         let document_view_id: DocumentViewId = document_id.as_str().parse().unwrap();
//         let author_performing_update = key_pair.public_key();
//
//         let update_operation = OperationBuilder::new(schema.id())
//             .action(OperationAction::Update)
//             .previous(&document_view_id)
//             .fields(&test_fields())
//             .build()
//             .unwrap();
//
//         let latest_entry = store
//             .get_latest_entry(&author_performing_update, &log_id)
//             .await
//             .unwrap();
//
//         let encoded_operation = encode_operation(&update_operation).unwrap();
//         let encoded_entry = sign_and_encode_entry(
//             &log_id,
//             &latest_entry
//                 .as_ref()
//                 .map(|entry| entry.seq_num().clone().next().unwrap())
//                 .unwrap_or_default(),
//             None,
//             latest_entry.map(|entry| entry.hash()).as_ref(),
//             &encoded_operation,
//             &key_pair,
//         )
//         .unwrap();
//
//         let result = publish(
//             &store,
//             &schema,
//             &encoded_entry.clone(),
//             &decode_operation(&encoded_operation).unwrap(),
//             &encoded_operation,
//         )
//         .await;
//
//         // The test will panic here when there is an error
//         result.map_err(|err| err.to_string()).unwrap();
//
//         // For non error cases we test that there is a log for the updated document.
//         let log = store
//             .get_log_id(&author_performing_update, document_id)
//             .await
//             .unwrap();
//
//         assert!(log.is_some());
//         assert_eq!(log.unwrap(), LogId::new(0));
//     }
//
//     #[rstest]
//     #[case::owner_publishes_to_next_log(
//         LogId::new(2),
//         KeyPair::from_private_key_str(PRIVATE_KEY).unwrap())
//     ]
//     #[case::owner_publishes_to_not_next_log(
//         LogId::new(100),
//         KeyPair::from_private_key_str(PRIVATE_KEY).unwrap())
//     ]
//     #[case::new_author_publishes_to_next_log(LogId::new(0), KeyPair::new())]
//     #[case::new_author_publishes_to_not_next_log(LogId::new(10), KeyPair::new())]
//     #[should_panic(
//         expected = "Entry's claimed seq num of 1 does not match expected seq num of 2 for given public key and log"
//     )]
//     #[case::owner_publishes_to_occupied_log(
//         LogId::new(1),
//         KeyPair::from_private_key_str(PRIVATE_KEY).unwrap())
//     ]
//     #[tokio::test]
//     async fn creating_new_document_inserts_log_correctly(
//         schema: Schema,
//         #[case] log_id: LogId,
//         #[case] key_pair: KeyPair,
//         operation: Operation,
//         #[from(populate_store_config)]
//         #[with(1, 2, 1)]
//         config: PopulateStoreConfig,
//     ) {
//         let store = MemoryStore::default();
//         let _ = populate_store(&store, &config).await;
//
//         // Construct and publish a new entry with the passed log id.
//         // The contained operation is a CREATE.
//         let encoded_operation = encode_operation(&operation).unwrap();
//         let encoded_entry = sign_and_encode_entry(
//             &log_id,
//             &SeqNum::default(),
//             None,
//             None,
//             &encoded_operation,
//             &key_pair,
//         )
//         .unwrap();
//
//         // This will error (and panic as we unwrap) if the claimed log id is incorrect.
//         // We test the error string is correct.
//         let _result = publish(
//             &store,
//             &schema,
//             &encoded_entry,
//             &decode_operation(&encoded_operation).unwrap(),
//             &encoded_operation,
//         )
//         .await
//         .map_err(|err| err.to_string())
//         .unwrap();
//
//         // If it didn't error the request succeeded, we check a new log was stored.
//         let public_key = key_pair.public_key();
//         let document_id = encoded_entry.hash().into();
//
//         let retrieved_log_id = store
//             .get_log_id(&public_key, &document_id)
//             .await
//             .expect("Retrieve log id for document");
//
//         assert_eq!(log_id, retrieved_log_id.unwrap())
//     }
//
//     #[rstest]
//     #[should_panic(
//         expected = "You are trying to update or delete a document which has been deleted"
//     )]
//     #[case(KeyPair::from_private_key_str(PRIVATE_KEY).unwrap())]
//     #[should_panic(
//         expected = "You are trying to update or delete a document which has been deleted"
//     )]
//     #[case(KeyPair::new())]
//     #[tokio::test]
//     async fn validates_that_document_is_deleted(
//         schema: Schema,
//         #[case] key_pair: KeyPair,
//         #[from(populate_store_config)]
//         #[with(2, 1, 1, true)]
//         config: PopulateStoreConfig,
//     ) {
//         let store = MemoryStore::default();
//         let (_, documents) = populate_store(&store, &config).await;
//
//         let document_id = documents.first().unwrap();
//         let document_view_id: DocumentViewId = document_id.as_str().parse().unwrap();
//         let author_performing_update = key_pair.public_key();
//
//         let delete_operation = OperationBuilder::new(schema.id())
//             .action(OperationAction::Delete)
//             .previous(&document_view_id)
//             .build()
//             .unwrap();
//
//         let latest_entry = store
//             .get_latest_entry(&author_performing_update, &LogId::default())
//             .await
//             .unwrap();
//
//         let encoded_operation = encode_operation(&delete_operation).unwrap();
//         let encoded_entry = sign_and_encode_entry(
//             &LogId::default(),
//             &latest_entry
//                 .as_ref()
//                 .map(|entry| entry.seq_num().clone().next().unwrap())
//                 .unwrap_or_default(),
//             None,
//             latest_entry.map(|entry| entry.hash()).as_ref(),
//             &encoded_operation,
//             &key_pair,
//         )
//         .unwrap();
//
//         let result = publish(
//             &store,
//             &schema,
//             &encoded_entry.clone(),
//             &decode_operation(&encoded_operation).unwrap(),
//             &encoded_operation,
//         )
//         .await;
//
//         result.map_err(|err| err.to_string()).unwrap();
//     }
//
//     #[rstest]
//     #[tokio::test]
//     async fn publish_many_entries(
//         #[with(vec![("name".to_string(), FieldType::String)])] schema: Schema,
//         key_pair: KeyPair,
//     ) {
//         let store = MemoryStore::default();
//
//         let num_of_entries = 13;
//         let mut document_id: Option<DocumentId> = None;
//         let public_key = key_pair.public_key();
//
//         for index in 0..num_of_entries {
//             let document_view_id: Option<DocumentViewId> =
//                 document_id.clone().map(|id| id.as_str().parse().unwrap());
//
//             let (backlink, skiplink, seq_num, log_id) =
//                 next_args(&store, &public_key, document_view_id.as_ref())
//                     .await
//                     .unwrap();
//
//             let schema_id = schema.id().to_owned();
//             let operation = if index == 0 {
//                 create_operation(
//                     vec![("name", OperationValue::String("Panda".to_string()))],
//                     schema_id,
//                 )
//             } else if index == (num_of_entries - 1) {
//                 delete_operation(backlink.clone().unwrap().into(), schema_id)
//             } else {
//                 update_operation(
//                     vec![("name", OperationValue::String("🐼".to_string()))],
//                     backlink.clone().unwrap().into(),
//                     schema_id,
//                 )
//             };
//
//             let encoded_operation = encode_operation(&operation).unwrap();
//             let encoded_entry = sign_and_encode_entry(
//                 &log_id,
//                 &seq_num,
//                 skiplink.map(Hash::from).as_ref(),
//                 backlink.map(Hash::from).as_ref(),
//                 &encoded_operation,
//                 &key_pair,
//             )
//             .unwrap();
//
//             if index == 0 {
//                 document_id = Some(encoded_entry.hash().into());
//             }
//
//             let result = publish(
//                 &store,
//                 &schema,
//                 &encoded_entry.clone(),
//                 &decode_operation(&encoded_operation).unwrap(),
//                 &encoded_operation,
//             )
//             .await;
//
//             assert!(result.is_ok());
//
//             let (_, _, next_seq_num, _) = result.unwrap();
//             let mut previous_seq_num = seq_num;
//
//             assert_eq!(next_seq_num, previous_seq_num.next().unwrap());
//             assert_eq!(log_id, LogId::default());
//         }
//     }
//
//     #[rstest]
//     #[should_panic(
//         expected = "Max sequence number reached for public key 2f8e50c2ede6d936ecc3144187ff1c273808185cfbc5ff3d3748d1ff7353fc96 log 0"
//     )]
//     #[tokio::test]
//     async fn validates_max_seq_num_reached(
//         schema: Schema,
//         key_pair: KeyPair,
//         #[from(populate_store_config)]
//         #[with(2, 1, 1, false)]
//         config: PopulateStoreConfig,
//     ) {
//         let store = MemoryStore::default();
//         let _ = populate_store(&store, &config).await;
//
//         let public_key = key_pair.public_key();
//
//         // Get the latest entry, we will use it's operation in all other entries (doesn't matter if it's a duplicate, just need the previous
//         // operations to exist).
//         let entry_two = store
//             .get_entry_at_seq_num(&public_key, &LogId::default(), &SeqNum::new(2).unwrap())
//             .await
//             .unwrap()
//             .unwrap();
//
//         // Create and insert the skiplink for MAX_SEQ_NUM entry
//
//         let encoded_entry = sign_and_encode_entry(
//             &LogId::default(),
//             &SeqNum::new(18446744073709551611).unwrap(),
//             Some(&random_hash()),
//             Some(&random_hash()),
//             entry_two.payload.as_ref().unwrap(),
//             &key_pair,
//         )
//         .unwrap();
//
//         let skiplink = StorageEntry::new(&encoded_entry, entry_two.payload.as_ref());
//         store
//             .entries
//             .lock()
//             .unwrap()
//             .insert(skiplink.hash(), skiplink.clone());
//
//         // Create and insert the backlink for MAX_SEQ_NUM entry
//         let encoded_entry = sign_and_encode_entry(
//             &LogId::default(),
//             &SeqNum::new(u64::MAX - 1).unwrap(),
//             None,
//             Some(&random_hash()),
//             entry_two.payload.as_ref().unwrap(),
//             &key_pair,
//         )
//         .unwrap();
//
//         let backlink = StorageEntry::new(&encoded_entry, entry_two.payload.as_ref());
//         store
//             .entries
//             .lock()
//             .unwrap()
//             .insert(backlink.hash(), backlink.clone());
//
//         // Create the MAX_SEQ_NUM entry using the above skiplink and backlink
//         let encoded_entry = sign_and_encode_entry(
//             &LogId::default(),
//             &SeqNum::new(u64::MAX).unwrap(),
//             Some(&skiplink.hash()),
//             Some(&backlink.hash()),
//             entry_two.payload.as_ref().unwrap(),
//             &key_pair,
//         )
//         .unwrap();
//
//         // Publish the MAX_SEQ_NUM entry
//         let operation = &entry_two.payload.unwrap();
//         let result = publish(
//             &store,
//             &schema,
//             &encoded_entry.clone(),
//             &decode_operation(operation).unwrap(),
//             operation,
//         )
//         .await;
//
//         // try and get the MAX_SEQ_NUM entry again (it shouldn't be there)
//         let entry_at_max_seq_num = store.get_entry(&encoded_entry.hash()).await.unwrap();
//
//         // We expect the entry we published not to have been stored in the db
//         assert!(entry_at_max_seq_num.is_none());
//         result.map_err(|err| err.to_string()).unwrap();
//     }
//
//     #[rstest]
//     #[should_panic(
//         expected = "Operation 00207f8ffabff270f21098a457b900b4989b7272a6cb637f3c938b06be0a77b708ed claims incorrect schema my_wrong_schema_name_"
//     )]
//     #[tokio::test]
//     async fn validates_incorrect_schema_id_in_previous_operation(
//         #[from(populate_store_config)]
//         #[with(1, 1, 1, false)]
//         config: PopulateStoreConfig,
//     ) {
//         let store = MemoryStore::default();
//         let (key_pairs, document_ids) = populate_store(&store, &config).await;
//
//         let document_id = document_ids.get(0).unwrap().to_owned();
//         let key_pair = key_pairs.get(0).unwrap().to_owned();
//
//         let create_view_id: DocumentViewId = document_id.as_str().parse().unwrap();
//
//         // A different schema from the operation already published to the store.
//         let schema_name = SchemaName::new("my_wrong_schema_name").expect("Valid schema name");
//         let schema_id = SchemaId::new_application(&schema_name, &random_document_view_id());
//         let schema = schema(
//             vec![("age".into(), FieldType::Integer)],
//             schema_id.clone(),
//             "Schema with different id",
//         );
//
//         // Create an operation correctly following the incorrect schema.
//         let update_with_different_schema_id = update_operation(
//             vec![("age", OperationValue::Integer(21))],
//             create_view_id,
//             schema_id,
//         );
//
//         // If we publish this operation it should fail as it's claimed schema is different from
//         // the one it points to in it's previous operations.
//         let result =
//             send_to_store(&store, &update_with_different_schema_id, &schema, key_pair).await;
//
//         result.map_err(|err| err.to_string()).unwrap();
//     }
// }
