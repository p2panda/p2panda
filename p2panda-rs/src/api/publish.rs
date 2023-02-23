// SPDX-License-Identifier: AGPL-3.0-or-later

use bamboo_rs_core_ed25519_yasmf::entry::is_lipmaa_required;

use crate::api::validation::{
    ensure_document_not_deleted, get_checked_document_id_for_view_id, get_expected_skiplink,
    increment_seq_num, is_next_seq_num, verify_log_id,
};
use crate::api::DomainError;
use crate::document::DocumentId;
use crate::entry::decode::decode_entry;
use crate::entry::traits::{AsEncodedEntry, AsEntry};
use crate::entry::{EncodedEntry, LogId, SeqNum};
use crate::hash::Hash;
use crate::identity::PublicKey;
use crate::operation::plain::PlainOperation;
use crate::operation::traits::AsOperation;
use crate::operation::validate::validate_operation_with_entry;
use crate::operation::{EncodedOperation, Operation, OperationAction, OperationId};
use crate::schema::Schema;
use crate::storage_provider::traits::{EntryStore, LogStore, OperationStore};

/// An entries' backlink returned by next_args.
type Backlink = Hash;

/// An entries' skiplink returned by next_args.
type Skiplink = Hash;

/// Persist an entry and operation to storage after performing validation of claimed values against
/// expected values retrieved from storage.
///
/// Returns the arguments required for constructing the next entry in a bamboo log for the
/// specified public key and document.
///
/// This method is intended to be used behind a public API and so we assume all passed values are
/// in themselves valid.
///
/// # Validation Steps Performed
///
/// Following is a list of the steps and validation checks that this method performs.
///
/// ## Validate Entry
///
/// Validate the values encoded on entry against what we expect based on our existing stored
/// entries:
///
/// - Verify the claimed sequence number against the expected next sequence number for the public key
/// and log.
/// - Get the expected backlink from storage.
/// - Get the expected skiplink from storage.
/// - Verify the bamboo entry (requires the expected backlink and skiplink to do this).
///
/// ## Validate operation against it's claimed schema:
///
/// - verify the content of an operation against it's stated schema.
///
/// ## Determine document id
///
/// - If this is a create operation:
///   - derive the document id from the entry hash.
/// - In all other cases:
///   - verify that all operations in previous exist in the database,
///   - verify that all operations in previous are from the same document,
///   - ensure that the document is not deleted.
/// - Verify that the claimed log id matches the expected log id for this public key and log.
///
/// ## Persist data
///
/// - If this is a new document:
///   - Store the new log.
/// - Store the entry.
/// - Store the operation.
///
/// ## Compute and return next entry arguments
///
/// - Done!
pub async fn publish<S: EntryStore + OperationStore + LogStore>(
    store: &S,
    schema: &Schema,
    encoded_entry: &EncodedEntry,
    plain_operation: &PlainOperation,
    encoded_operation: &EncodedOperation,
) -> Result<(Option<Backlink>, Option<Skiplink>, SeqNum, LogId), DomainError> {
    // Decode the entry.
    let entry = decode_entry(encoded_entry)?;

    // Validate the entry and operation.
    let (operation, operation_id) = validate_entry_and_operation(
        store,
        schema,
        &entry,
        encoded_entry,
        plain_operation,
        encoded_operation,
    )
    .await?;

    // Determine the document id.
    let document_id = determine_document_id(store, &operation, &operation_id).await?;

    // Verify the claimed log id against the expected one for this document id and public_key.
    verify_log_id(store, entry.public_key(), entry.log_id(), &document_id).await?;

    // If we have reached MAX_SEQ_NUM here for the next args then we will error and _not_ store
    // the entry which is being processed in this request.
    let next_seq_num = increment_seq_num(&mut entry.seq_num().clone()).map_err(|_| {
        DomainError::MaxSeqNumReached(entry.public_key().to_string(), entry.log_id().as_u64())
    })?;

    // Get the skiplink for the following entry to be used in next args
    let skiplink =
        get_skiplink_for_entry(store, &next_seq_num, entry.log_id(), entry.public_key()).await?;

    ///////////////
    // STORE LOG //
    ///////////////

    // If the entries' seq num is 1 we insert a new log here.
    if entry.seq_num().is_first() {
        store
            .insert_log(
                entry.log_id(),
                entry.public_key(),
                &operation.schema_id(),
                &document_id,
            )
            .await?;
    }

    ///////////////////////////////
    // STORE ENTRY AND OPERATION //
    ///////////////////////////////

    // Insert the entry into the store.
    store
        .insert_entry(&entry, encoded_entry, Some(encoded_operation))
        .await?;

    // Insert the operation into the store.
    store
        .insert_operation(&operation_id, entry.public_key(), &operation, &document_id)
        .await?;

    // Construct and return next args.
    Ok((
        Some(encoded_entry.hash()),
        skiplink,
        next_seq_num,
        entry.log_id().to_owned(),
    ))
}

/// Wrapper for `operation::validate::validate_operation_with_entry` which makes use of methods
/// provided by `storage_traits` in order to fetch values from the store which are required for
/// performing the following validation steps. See
/// `operation::validate::validate_operation_with_entry` for detailed explanation of the steps taken.
async fn validate_entry_and_operation<S: EntryStore + OperationStore + LogStore>(
    store: &S,
    schema: &Schema,
    entry: &impl AsEntry,
    encoded_entry: &impl AsEncodedEntry,
    plain_operation: &PlainOperation,
    encoded_operation: &EncodedOperation,
) -> Result<(Operation, OperationId), DomainError> {
    // Verify that the claimed seq num matches the expected seq num for this public_key and log.
    let latest_entry = store
        .get_latest_entry(entry.public_key(), entry.log_id())
        .await?;
    let latest_seq_num = latest_entry.as_ref().map(|entry| entry.seq_num());
    is_next_seq_num(latest_seq_num, entry.seq_num())?;

    // If a skiplink is claimed, get the expected skiplink from the database, errors if it can't be found.
    let skiplink = match entry.skiplink() {
        Some(_) => Some(
            get_expected_skiplink(store, entry.public_key(), entry.log_id(), entry.seq_num())
                .await?,
        ),
        None => None,
    };

    // Construct params as `validate_operation_with_entry` expects.
    let skiplink_params = skiplink.as_ref().map(|entry| {
        let hash = entry.hash();
        (entry.clone(), hash)
    });

    // The backlink for this entry is the latest entry from this public key's log.
    let backlink_params = latest_entry.as_ref().map(|entry| {
        let hash = entry.hash();
        (entry.clone(), hash)
    });

    // Perform validation of the entry and it's operation.
    let (operation, operation_id) = validate_operation_with_entry(
        entry,
        encoded_entry,
        skiplink_params.as_ref().map(|(entry, hash)| (entry, hash)),
        backlink_params.as_ref().map(|(entry, hash)| (entry, hash)),
        plain_operation,
        encoded_operation,
        schema,
    )?;

    Ok((operation, operation_id))
}

/// Determine the document id for the passed operation. If this is a create operation then we use
/// the provided operation id to derive a new document id. In all other cases we retrieve and
/// validate the document id by look at the operations contained in the `previous` field.  
async fn determine_document_id<S: EntryStore + OperationStore + LogStore>(
    store: &S,
    operation: &Operation,
    operation_id: &OperationId,
) -> Result<DocumentId, DomainError> {
    match operation.action() {
        OperationAction::Create => {
            // Derive the document id for this new document.
            Ok(DocumentId::new(operation_id))
        }
        _ => {
            // We can unwrap previous operations here as we know all UPDATE and DELETE operations contain them.
            let previous = operation.previous().unwrap();

            // Get the document_id for the document_view_id contained in previous operations.
            // This performs several validation steps (check method doc string).
            let document_id = get_checked_document_id_for_view_id(store, &previous).await?;

            // Ensure the document isn't deleted.
            ensure_document_not_deleted(store, &document_id)
                .await
                .map_err(|_| DomainError::DeletedDocument)?;

            Ok(document_id)
        }
    }
}

/// Retrieve the expected skiplink for the entry identified by public key, log id and sequence number.
async fn get_skiplink_for_entry<S: EntryStore + OperationStore + LogStore>(
    store: &S,
    seq_num: &SeqNum,
    log_id: &LogId,
    public_key: &PublicKey,
) -> Result<Option<Skiplink>, DomainError> {
    // Check if skiplink is required and return hash if so
    let skiplink = if is_lipmaa_required(seq_num.as_u64()) {
        Some(get_expected_skiplink(store, public_key, log_id, seq_num).await?)
    } else {
        None
    }
    .map(|entry| entry.hash());

    Ok(skiplink)
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use crate::api::{next_args, publish};
    use crate::document::{DocumentId, DocumentViewId};
    use crate::entry::encode::sign_and_encode_entry;
    use crate::entry::traits::{AsEncodedEntry, AsEntry};
    use crate::entry::{LogId, SeqNum};
    use crate::hash::Hash;
    use crate::identity::KeyPair;
    use crate::operation::decode::decode_operation;
    use crate::operation::encode::encode_operation;
    use crate::operation::{
        Operation, OperationAction, OperationBuilder, OperationId, OperationValue,
    };
    use crate::schema::{FieldType, Schema};
    use crate::storage_provider::traits::{EntryStore, LogStore};
    use crate::test_utils::constants::{test_fields, PRIVATE_KEY};
    use crate::test_utils::fixtures::populate_store_config;
    use crate::test_utils::fixtures::{
        create_operation, delete_operation, key_pair, operation, random_hash, schema,
        update_operation,
    };
    use crate::test_utils::memory_store::helpers::{
        populate_store, remove_entries, remove_operations, PopulateStoreConfig,
    };
    use crate::test_utils::memory_store::{MemoryStore, StorageEntry};

    type LogIdAndSeqNum = (u64, u64);

    #[rstest]
    #[case::ok(&[(0, 8)], (0, 8))]
    #[should_panic(
        expected = "Expected skiplink entry not found in store: public key 2f8e50c2ede6d936ecc3144187ff1c273808185cfbc5ff3d3748d1ff7353fc96, log id 0, seq num 4"
    )]
    #[case::skiplink_missing(&[(0, 4), (0, 8)], (0, 8))]
    #[should_panic(
        expected = "Entry's claimed seq num of 8 does not match expected seq num of 7 for given public key and log"
    )]
    #[case::backlink_missing(&[(0, 7), (0, 8)], (0, 8))]
    #[should_panic(
        expected = "Entry's claimed seq num of 8 does not match expected seq num of 7 for given public key and log"
    )]
    #[case::backlink_and_skiplink_missing(&[(0, 4), (0, 7), (0, 8)], (0, 8))]
    #[should_panic(
        expected = "Entry's claimed seq num of 8 does not match expected seq num of 9 for given public key and log"
    )]
    #[case::seq_num_occupied_again(&[], (0, 8))]
    #[should_panic(
        expected = "Entry's claimed seq num of 7 does not match expected seq num of 9 for given public key and log"
    )]
    #[case::seq_num_occupied_(&[], (0, 7))]
    #[should_panic(
        expected = "Expected skiplink entry not found in store: public key 2f8e50c2ede6d936ecc3144187ff1c273808185cfbc5ff3d3748d1ff7353fc96, log id 0, seq num 4"
    )]
    #[case::next_args_skiplink_missing(&[(0, 4), (0, 7), (0, 8)], (0, 7))]
    #[should_panic(
        expected = "Entry's claimed seq num of 8 does not match expected seq num of 1 for given public key and log"
    )]
    #[case::no_entries_yet(&[(0, 1), (0, 2), (0, 3), (0, 4), (0, 5), (0, 6), (0, 7), (0, 8)], (0, 8))]
    #[tokio::test]
    async fn publish_with_missing_entries(
        schema: Schema,
        #[case] entries_to_remove: &[LogIdAndSeqNum],
        #[case] entry_to_publish: LogIdAndSeqNum,
        #[from(populate_store_config)]
        #[with(8, 1, 1)]
        config: PopulateStoreConfig,
    ) {
        let store = MemoryStore::default();
        let (key_pairs, _) = populate_store(&store, &config).await;

        // The public key who has published to the db.
        let public_key = key_pairs[0].public_key();

        // Get the latest entry from the db.
        let next_entry = store
            .get_entry_at_seq_num(
                &public_key,
                &LogId::new(entry_to_publish.0),
                &SeqNum::new(entry_to_publish.1).unwrap(),
            )
            .await
            .unwrap()
            .unwrap();

        // Remove some entries and operations from the database.
        remove_operations(&store, &public_key, entries_to_remove);
        remove_entries(&store, &public_key, entries_to_remove);

        // Publish the latest entry again and see what happens.
        let operation = next_entry.payload.unwrap();
        let result = publish(
            &store,
            &schema,
            &next_entry.encoded_entry,
            &decode_operation(&operation).unwrap(),
            &operation,
        )
        .await;

        // Unwrap here causing a panic, we check the errors match what we expect.
        result.map_err(|err| err.to_string()).unwrap();
    }

    #[rstest]
    #[case::ok_single_writer(
        &[],
        &[(0, 8)],
        KeyPair::from_private_key_str(PRIVATE_KEY).unwrap()
    )]
    // Weird case where all previous operations are on the same branch, but still valid.
    #[case::ok_many_previous(
        &[],
        &[(0, 8), (0, 7), (0, 6)],
        KeyPair::from_private_key_str(PRIVATE_KEY).unwrap()
    )]
    #[case::ok_multi_writer(
        &[],
        &[(0, 8)],
        KeyPair::new()
    )]
    #[should_panic(
        expected = "Operation 00209038901221ce1002f023461f1530adf632081d9fcd2da1082c7c91fdcb534d03 not found, could not determine document id"
    )]
    #[case::previous_operation_missing(
        &[(0, 8)],
        &[(0, 8)],
        KeyPair::from_private_key_str(PRIVATE_KEY).unwrap()
    )]
    #[should_panic(
        expected = "Operation 00201971f1257645a2f6d3465f8713991d269709f81a5c6c458168b9461d68af5ecf not found, could not determine document id"
    )]
    #[case::one_of_some_previous_missing(
        &[(0, 7)],
        &[(0, 7), (0, 8)],
        KeyPair::from_private_key_str(PRIVATE_KEY).unwrap()
    )]
    #[should_panic(
        expected = "Operation 00209038901221ce1002f023461f1530adf632081d9fcd2da1082c7c91fdcb534d03 not found, could not determine document id"
    )]
    #[case::one_of_some_previous_missing(
        &[(0, 8)],
        &[(0, 7), (0, 8)],
        KeyPair::from_private_key_str(PRIVATE_KEY).unwrap()
    )]
    #[should_panic(
        expected = "Operation 00209038901221ce1002f023461f1530adf632081d9fcd2da1082c7c91fdcb534d03 not found, could not determine document id"
    )]
    #[case::missing_previous_operation_multi_writer(
        &[(0, 8)],
        &[(0, 8)],
        KeyPair::new()
    )]
    #[should_panic(
        expected = "Operations in passed document view id originate from different documents"
    )]
    #[case::previous_invalid_multiple_document_id(
        &[],
        &[(0, 8), (1, 8)],
        KeyPair::from_private_key_str(PRIVATE_KEY).unwrap()
    )]
    #[tokio::test]
    async fn publish_with_missing_operations(
        schema: Schema,
        // The operations to be removed from the db
        #[case] operations_to_remove: &[LogIdAndSeqNum],
        // The previous operations described by their log id and seq number (log_id, seq_num)
        #[case] previous: &[LogIdAndSeqNum],
        #[case] key_pair: KeyPair,
        #[from(populate_store_config)]
        #[with(8, 2, 1)]
        config: PopulateStoreConfig,
    ) {
        let store = MemoryStore::default();
        let (key_pairs, documents) = populate_store(&store, &config).await;

        let existing_author = key_pairs[0].public_key();

        // Get the document id.
        let document = documents.first().map(|id| id.as_str().parse().unwrap());

        // Map the passed &[LogIdAndSeqNum] into a DocumentViewId containing the claimed operations.
        let previous: Vec<OperationId> = previous
            .iter()
            .filter_map(|(log_id, seq_num)| {
                store
                    .entries
                    .lock()
                    .unwrap()
                    .values()
                    .find(|entry| {
                        entry.seq_num().as_u64() == *seq_num
                            && entry.log_id().as_u64() == *log_id
                            && *entry.public_key() == existing_author
                    })
                    .map(|entry| entry.hash().into())
            })
            .collect();

        // Construct document view id for previous operations.
        let document_view_id = DocumentViewId::new(&previous);

        // Compose the next operation.
        let next_operation = OperationBuilder::new(schema.id())
            .action(OperationAction::Update)
            .previous(&document_view_id)
            .fields(&test_fields())
            .build()
            .unwrap();

        // The next args for a author who will publish the next entry based on
        // the passed key pair for this test run.
        let (backlink, skiplink, seq_num, log_id) =
            next_args(&store, &key_pair.public_key(), document.as_ref())
                .await
                .unwrap();

        let encoded_operation = encode_operation(&next_operation).unwrap();
        let encoded_entry = sign_and_encode_entry(
            &log_id,
            &seq_num,
            skiplink.map(Hash::from).as_ref(),
            backlink.map(Hash::from).as_ref(),
            &encoded_operation,
            &key_pair,
        )
        .unwrap();

        // Remove some entries from the db.
        remove_operations(&store, &existing_author, operations_to_remove);

        // Publish the entry and operation.
        let result = publish(
            &store,
            &schema,
            &encoded_entry,
            &decode_operation(&encoded_operation).unwrap(),
            &encoded_operation,
        )
        .await;

        // Unwrap here causing a panic, we check the errors match what we expect.
        result.map_err(|err| err.to_string()).unwrap();
    }

    #[rstest]
    #[case::owner_publishes_update_to_correct_log(
        LogId::new(0),
        KeyPair::from_private_key_str(PRIVATE_KEY).unwrap())
    ]
    #[case::new_author_updates_to_new_log(LogId::new(0), KeyPair::new())]
    #[should_panic(
        expected = "Entry's claimed log id of 1 does not match existing log id of 0 for given public key and document"
    )]
    #[case::owner_updates_to_wrong_and_taken_log(LogId::new(1), KeyPair::from_private_key_str(PRIVATE_KEY).unwrap())]
    #[should_panic(
        expected = "Entry's claimed log id of 2 does not match existing log id of 0 for given public key and document"
    )]
    #[case::owner_updates_to_wrong_but_free_log(LogId::new(2), KeyPair::from_private_key_str(PRIVATE_KEY).unwrap())]
    #[should_panic(
        expected = "Entry's claimed log id of 1 does not match expected next log id of 0 for given public key"
    )]
    #[case::new_author_updates_to_wrong_new_log(LogId::new(1), KeyPair::new())]
    #[tokio::test]
    async fn publish_update_log_tests(
        schema: Schema,
        #[case] log_id: LogId,
        #[case] key_pair: KeyPair,
        #[from(populate_store_config)]
        #[with(2, 1, 1)]
        config: PopulateStoreConfig,
    ) {
        let store = MemoryStore::default();
        let (_, documents) = populate_store(&store, &config).await;

        let document_id = documents.first().unwrap();
        let document_view_id: DocumentViewId = document_id.as_str().parse().unwrap();
        let author_performing_update = key_pair.public_key();

        let update_operation = OperationBuilder::new(schema.id())
            .action(OperationAction::Update)
            .previous(&document_view_id)
            .fields(&test_fields())
            .build()
            .unwrap();

        let latest_entry = store
            .get_latest_entry(&author_performing_update, &log_id)
            .await
            .unwrap();

        let encoded_operation = encode_operation(&update_operation).unwrap();
        let encoded_entry = sign_and_encode_entry(
            &log_id,
            &latest_entry
                .as_ref()
                .map(|entry| entry.seq_num().clone().next().unwrap())
                .unwrap_or_default(),
            None,
            latest_entry.map(|entry| entry.hash()).as_ref(),
            &encoded_operation,
            &key_pair,
        )
        .unwrap();

        let result = publish(
            &store,
            &schema,
            &encoded_entry.clone(),
            &decode_operation(&encoded_operation).unwrap(),
            &encoded_operation,
        )
        .await;

        // The test will panic here when there is an error
        result.map_err(|err| err.to_string()).unwrap();

        // For non error cases we test that there is a log for the updated document.
        let log = store
            .get_log_id(&author_performing_update, document_id)
            .await
            .unwrap();

        assert!(log.is_some());
        assert_eq!(log.unwrap(), LogId::new(0));
    }

    #[rstest]
    #[case::owner_publishes_to_correct_log(
        LogId::new(2),
        KeyPair::from_private_key_str(PRIVATE_KEY).unwrap())
    ]
    #[case::new_author_publishes_to_new_log(LogId::new(0), KeyPair::new())]
    #[should_panic(
        expected = "Entry's claimed seq num of 1 does not match expected seq num of 2 for given public key and log"
    )]
    #[case::owner_publishes_to_wrong_and_taken_log(
        LogId::new(1),
        KeyPair::from_private_key_str(PRIVATE_KEY).unwrap())
    ]
    #[should_panic(
        expected = "Entry's claimed log id of 3 does not match expected next log id of 2 for given public key"
    )]
    #[case::owner_publishes_to_wrong_but_free_log(
        LogId::new(3),
        KeyPair::from_private_key_str(PRIVATE_KEY).unwrap())
    ]
    #[should_panic(
        expected = "Entry's claimed log id of 1 does not match expected next log id of 0 for given public key"
    )]
    #[case::new_author_publishes_to_wrong_new_log(LogId::new(1), KeyPair::new())]
    #[tokio::test]
    async fn publish_create_log_tests(
        schema: Schema,
        #[case] log_id: LogId,
        #[case] key_pair: KeyPair,
        operation: Operation,
        #[from(populate_store_config)]
        #[with(1, 2, 1)]
        config: PopulateStoreConfig,
    ) {
        let store = MemoryStore::default();
        let _ = populate_store(&store, &config).await;

        // Construct and publish a new entry with the passed log id.
        let encoded_operation = encode_operation(&operation).unwrap();
        let encoded_entry = sign_and_encode_entry(
            &log_id,
            &SeqNum::default(),
            None,
            None,
            &encoded_operation,
            &key_pair,
        )
        .unwrap();

        // This will error (and panic as we unwrap) if the claimed log id is incorrect.
        // We test the error string is correct.
        let _result = publish(
            &store,
            &schema,
            &encoded_entry,
            &decode_operation(&encoded_operation).unwrap(),
            &encoded_operation,
        )
        .await
        .map_err(|err| err.to_string())
        .unwrap();

        // If it didn't error the request succeeded, we check a new log was stored.
        let public_key = key_pair.public_key();
        let document_id = encoded_entry.hash().into();

        let retrieved_log_id = store
            .get_log_id(&public_key, &document_id)
            .await
            .expect("Retrieve log id for document");

        assert_eq!(log_id, retrieved_log_id.unwrap())
    }

    #[rstest]
    #[should_panic(
        expected = "You are trying to update or delete a document which has been deleted"
    )]
    #[case(KeyPair::from_private_key_str(PRIVATE_KEY).unwrap())]
    #[should_panic(
        expected = "You are trying to update or delete a document which has been deleted"
    )]
    #[case(KeyPair::new())]
    #[tokio::test]
    async fn publish_to_deleted_documents(
        schema: Schema,
        #[case] key_pair: KeyPair,
        #[from(populate_store_config)]
        #[with(2, 1, 1, true)]
        config: PopulateStoreConfig,
    ) {
        let store = MemoryStore::default();
        let (_, documents) = populate_store(&store, &config).await;

        let document_id = documents.first().unwrap();
        let document_view_id: DocumentViewId = document_id.as_str().parse().unwrap();
        let author_performing_update = key_pair.public_key();

        let delete_operation = OperationBuilder::new(schema.id())
            .action(OperationAction::Delete)
            .previous(&document_view_id)
            .build()
            .unwrap();

        let latest_entry = store
            .get_latest_entry(&author_performing_update, &LogId::default())
            .await
            .unwrap();

        let encoded_operation = encode_operation(&delete_operation).unwrap();
        let encoded_entry = sign_and_encode_entry(
            &LogId::default(),
            &latest_entry
                .as_ref()
                .map(|entry| entry.seq_num().clone().next().unwrap())
                .unwrap_or_default(),
            None,
            latest_entry.map(|entry| entry.hash()).as_ref(),
            &encoded_operation,
            &key_pair,
        )
        .unwrap();

        let result = publish(
            &store,
            &schema,
            &encoded_entry.clone(),
            &decode_operation(&encoded_operation).unwrap(),
            &encoded_operation,
        )
        .await;

        result.map_err(|err| err.to_string()).unwrap();
    }

    #[rstest]
    #[tokio::test]
    async fn publish_many_entries(
        #[with(vec![("name".to_string(), FieldType::String)])] schema: Schema,
        key_pair: KeyPair,
    ) {
        let store = MemoryStore::default();

        let num_of_entries = 13;
        let mut document_id: Option<DocumentId> = None;
        let public_key = key_pair.public_key();

        for index in 0..num_of_entries {
            let document_view_id: Option<DocumentViewId> =
                document_id.clone().map(|id| id.as_str().parse().unwrap());

            let (backlink, skiplink, seq_num, log_id) =
                next_args(&store, &public_key, document_view_id.as_ref())
                    .await
                    .unwrap();

            let schema_id = schema.id().to_owned();
            let operation = if index == 0 {
                create_operation(
                    vec![("name", OperationValue::String("Panda".to_string()))],
                    schema_id,
                )
            } else if index == (num_of_entries - 1) {
                delete_operation(backlink.clone().unwrap().into(), schema_id)
            } else {
                update_operation(
                    vec![("name", OperationValue::String("🐼".to_string()))],
                    backlink.clone().unwrap().into(),
                    schema_id,
                )
            };

            let encoded_operation = encode_operation(&operation).unwrap();
            let encoded_entry = sign_and_encode_entry(
                &log_id,
                &seq_num,
                skiplink.map(Hash::from).as_ref(),
                backlink.map(Hash::from).as_ref(),
                &encoded_operation,
                &key_pair,
            )
            .unwrap();

            if index == 0 {
                document_id = Some(encoded_entry.hash().into());
            }

            let result = publish(
                &store,
                &schema,
                &encoded_entry.clone(),
                &decode_operation(&encoded_operation).unwrap(),
                &encoded_operation,
            )
            .await;

            assert!(result.is_ok());

            let (_, _, next_seq_num, _) = result.unwrap();
            let mut previous_seq_num = seq_num;

            assert_eq!(next_seq_num, previous_seq_num.next().unwrap());
            assert_eq!(log_id, LogId::default());
        }
    }

    #[rstest]
    #[should_panic(
        expected = "Max sequence number reached for public key 2f8e50c2ede6d936ecc3144187ff1c273808185cfbc5ff3d3748d1ff7353fc96 log 0"
    )]
    #[tokio::test]
    async fn publish_max_seq_num_reached(
        schema: Schema,
        key_pair: KeyPair,
        #[from(populate_store_config)]
        #[with(2, 1, 1, false)]
        config: PopulateStoreConfig,
    ) {
        let store = MemoryStore::default();
        let _ = populate_store(&store, &config).await;

        let public_key = key_pair.public_key();

        // Get the latest entry, we will use it's operation in all other entries (doesn't matter if it's a duplicate, just need the previous
        // operations to exist).
        let entry_two = store
            .get_entry_at_seq_num(&public_key, &LogId::default(), &SeqNum::new(2).unwrap())
            .await
            .unwrap()
            .unwrap();

        // Create and insert the skiplink for MAX_SEQ_NUM entry

        let encoded_entry = sign_and_encode_entry(
            &LogId::default(),
            &SeqNum::new(18446744073709551611).unwrap(),
            Some(&random_hash()),
            Some(&random_hash()),
            entry_two.payload.as_ref().unwrap(),
            &key_pair,
        )
        .unwrap();

        let skiplink = StorageEntry::new(&encoded_entry, entry_two.payload.as_ref());
        store
            .entries
            .lock()
            .unwrap()
            .insert(skiplink.hash(), skiplink.clone());

        // Create and insert the backlink for MAX_SEQ_NUM entry
        let encoded_entry = sign_and_encode_entry(
            &LogId::default(),
            &SeqNum::new(u64::MAX - 1).unwrap(),
            None,
            Some(&random_hash()),
            entry_two.payload.as_ref().unwrap(),
            &key_pair,
        )
        .unwrap();

        let backlink = StorageEntry::new(&encoded_entry, entry_two.payload.as_ref());
        store
            .entries
            .lock()
            .unwrap()
            .insert(backlink.hash(), backlink.clone());

        // Create the MAX_SEQ_NUM entry using the above skiplink and backlink
        let encoded_entry = sign_and_encode_entry(
            &LogId::default(),
            &SeqNum::new(u64::MAX).unwrap(),
            Some(&skiplink.hash()),
            Some(&backlink.hash()),
            entry_two.payload.as_ref().unwrap(),
            &key_pair,
        )
        .unwrap();

        // Publish the MAX_SEQ_NUM entry
        let operation = &entry_two.payload.unwrap();
        let result = publish(
            &store,
            &schema,
            &encoded_entry.clone(),
            &decode_operation(operation).unwrap(),
            operation,
        )
        .await;

        // try and get the MAX_SEQ_NUM entry again (it shouldn't be there)
        let entry_at_max_seq_num = store.get_entry(&encoded_entry.hash()).await.unwrap();

        // We expect the entry we published not to have been stored in the db
        assert!(entry_at_max_seq_num.is_none());
        result.map_err(|err| err.to_string()).unwrap();
    }
}
