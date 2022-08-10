// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::HashSet;

use crate::document::{DocumentId, DocumentViewId};
use crate::entry::decode::decode_entry;
use crate::entry::{EncodedEntry, LogId, SeqNum};
use crate::identity::Author;
use crate::operation::traits::{AsOperation, AsVerifiedOperation};
use crate::operation::{EncodedOperation, Operation, OperationAction};
use crate::storage_provider::traits::{AsStorageEntry, AsStorageLog, StorageProvider};
use crate::Human;
use bamboo_rs_core_ed25519_yasmf::entry::is_lipmaa_required;

use crate::storage_provider::validation::{
    ensure_document_not_deleted, get_expected_skiplink, increment_seq_num, is_next_seq_num,
    next_log_id, verify_log_id,
};

/// Retrieve arguments required for constructing the next entry in a bamboo log for a specific
/// author and document.
///
/// We accept a `DocumentViewId` rather than a `DocumentId` as an argument and then identify
/// the document id based on operations already existing in the store. Doing this means a document
/// can be updated without knowing the document id itself.
///
/// This method is intended to be used behind a public API and so we assume all passed values
/// are in themselves valid.
///
/// The steps and validation checks this method performs are:
///
/// Check if a document view id was passed
///
/// - if it wasn't we are creating a new document, safely increment the latest log id for the
///     passed author and return args immediately
/// - if it was continue knowing we are updating an existing document
///
/// Determine the document id we are concerned with
///
/// - verify that all operations in the passed document view id exist in the database
/// - verify that all operations in the passed document view id are from the same document
/// - ensure the document is not deleted
///
/// Determine next arguments
///
/// - get the log id for this author and document id, or if none is found safely increment this
///     authors latest log id
/// - get the backlink entry (latest entry for this author and log)
/// - get the skiplink for this author, log and next seq num
/// - get the latest seq num for this author and log and safely increment
///
/// Finally, return next arguments
pub async fn next_args<S: StorageProvider>(
    store: &S,
    public_key: &Author,
    document_view_id: Option<&DocumentViewId>,
) -> Result<NextEntryArguments> {
    // Init the next args with base default values.
    let mut next_args = NextEntryArguments {
        backlink: None,
        skiplink: None,
        seq_num: SeqNum::default().into(),
        log_id: LogId::default().into(),
    };

    ////////////////////////
    // HANDLE CREATE CASE //
    ////////////////////////

    // If no document_view_id is passed then this is a request for publishing a CREATE operation
    // and we return the args for the next free log by this author.
    if document_view_id.is_none() {
        let log_id = next_log_id(store, public_key).await?;
        next_args.log_id = log_id.into();
        return Ok(next_args);
    }

    ///////////////////////////
    // DETERMINE DOCUMENT ID //
    ///////////////////////////

    // We can unwrap here as we know document_view_id is some.
    let document_view_id = document_view_id.unwrap();

    // Get the document_id for this document_view_id. This performs several validation steps (check
    // method doc string).
    let document_id = get_checked_document_id_for_view_id(store, document_view_id).await?;

    // Check the document is not deleted.
    ensure_document_not_deleted(store, &document_id).await?;

    /////////////////////////
    // DETERMINE NEXT ARGS //
    /////////////////////////

    // Retrieve the log_id for the found document_id and author.
    //
    // @TODO: (lolz, this method is just called `get()`)
    let log_id = store.get(public_key, &document_id).await?;

    // Check if an existing log id was found for this author and document.
    match log_id {
        // If it wasn't found, we just calculate the next log id safely and return the next args.
        None => {
            let next_log_id = next_log_id(store, public_key).await?;
            next_args.log_id = next_log_id.into()
        }
        // If one was found, we need to get the backlink and skiplink, and safely increment the seq num.
        Some(log_id) => {
            // Get the latest entry in this log.
            let latest_entry = store.get_latest_entry(public_key, &log_id).await?;

            // Determine the next sequence number by incrementing one from the latest entry seq num.
            //
            // If the latest entry is None, then we must be at seq num 1.
            let seq_num = match latest_entry {
                Some(ref latest_entry) => {
                    let mut latest_seq_num = latest_entry.seq_num();
                    increment_seq_num(&mut latest_seq_num)
                }
                None => Ok(SeqNum::default()),
            }
            .map_err(|_| {
                anyhow!(
                    "Max sequence number reached for {} log {}",
                    public_key.display(),
                    log_id.as_u64()
                )
            })?;

            // Check if skiplink is required and if it is get the entry and return its hash.
            let skiplink = if is_lipmaa_required(seq_num.as_u64()) {
                // Determine skiplink ("lipmaa"-link) entry in this log.
                Some(get_expected_skiplink(store, public_key, &log_id, &seq_num).await?)
            } else {
                None
            }
            .map(|entry| entry.hash());

            next_args.backlink = latest_entry.map(|entry| entry.hash().into());
            next_args.skiplink = skiplink.map(|hash| hash.into());
            next_args.seq_num = seq_num.into();
            next_args.log_id = log_id.into();
        }
    };

    Ok(next_args)
}

/// Persist an entry and operation to storage after performing validation of claimed values against
/// expected values retrieved from storage.
///
/// Returns the arguments required for constructing the next entry in a bamboo log for the
/// specified author and document.
///
/// This method is intended to be used behind a public API and so we assume all passed values
/// are in themselves valid.
///
/// # Steps and Validation Performed
///
/// Following is a list of the steps and validation checks that this method performs.
///
/// ## Validate Entry
///
/// Validate the values encoded on entry against what we expect based on our existing stored
/// entries:
///
/// - Verify the claimed sequence number against the expected next sequence number for the author
///     and log.
/// - Get the expected backlink from storage.
/// - Get the expected skiplink from storage.
/// - Verify the bamboo entry (requires the expected backlink and skiplink to do this).
///
/// ## Ensure single node per author
///
/// - @TODO
///
/// ## Validate operation against it's claimed schema:
///
/// - @TODO
///
/// ## Determine document id
///
/// - If this is a create operation:
///   - derive the document id from the entry hash.
/// - In all other cases:
///   - verify that all operations in previous_operations exist in the database,
///   - verify that all operations in previous_operations are from the same document,
///   - ensure that the document is not deleted.
/// - Verify that the claimed log id matches the expected log id for this author and log.
///
/// ## Persist data
///
/// - If this is a new document:
///   - Store the new log.
/// - Store the entry.
/// - Store the operation.
///
/// ## Compute and return next entry arguments
pub async fn publish<S: StorageProvider>(
    store: &S,
    entry_encoded: &EncodedEntry,
    operation_encoded: &EncodedOperation,
) -> Result<NextEntryArguments> {
    ////////////////////////////////
    // DECODE ENTRY AND OPERATION //
    ////////////////////////////////

    let entry = decode_entry(entry_encoded, Some(operation_encoded))?;
    let operation = Operation::from(operation_encoded);
    let author = entry_encoded.author();
    let log_id = entry.log_id();
    let seq_num = entry.seq_num();

    ///////////////////////////
    // VALIDATE ENTRY VALUES //
    ///////////////////////////

    // Verify that the claimed seq num matches the expected seq num for this author and log.
    let latest_entry = store.get_latest_entry(&author, log_id).await?;
    let latest_seq_num = latest_entry.as_ref().map(|entry| entry.seq_num());
    is_next_seq_num(latest_seq_num.as_ref(), seq_num)?;

    // The backlink for this entry is the latest entry from this public key's log.
    let backlink = latest_entry;

    // If a skiplink is claimed, get the expected skiplink from the database, errors
    // if it can't be found.
    let skiplink = match entry.skiplink_hash() {
        Some(_) => Some(get_expected_skiplink(store, &author, log_id, seq_num).await?),
        None => None,
    };

    // Verify the bamboo entry providing the encoded operation and retrieved backlink and skiplink.
    bamboo_rs_core_ed25519_yasmf::verify(
        &entry_encoded.to_bytes(),
        Some(&operation_encoded.to_bytes()),
        skiplink.map(|entry| entry.entry_bytes()).as_deref(),
        backlink.map(|entry| entry.entry_bytes()).as_deref(),
    )?;

    ///////////////////////////////////
    // ENSURE SINGLE NODE PER AUTHOR //
    ///////////////////////////////////

    // @TODO: Missing a step here where we check if the author has published to this node before, and also
    // if we know of any other nodes they have published to. Not sure how to do this yet.

    ///////////////////////////////
    // VALIDATE OPERATION VALUES //
    ///////////////////////////////

    // @TODO: We skip this for now and will implement it in a follow-up PR
    // validate_operation_against_schema(store, operation.operation()).await?;

    //////////////////////////
    // DETERMINE DOCUMENT ID //
    //////////////////////////

    let document_id = match operation.action() {
        OperationAction::Create => {
            // Derive the document id for this new document.
            entry_encoded.hash().into()
        }
        _ => {
            // We can unwrap previous operations here as we know all UPDATE and DELETE operations contain them.
            let previous_operations = operation.previous_operations().unwrap();

            // Get the document_id for the document_view_id contained in previous operations.
            // This performs several validation steps (check method doc string).
            let document_id =
                get_checked_document_id_for_view_id(store, &previous_operations).await?;

            // Ensure the document isn't deleted.
            ensure_document_not_deleted(store, &document_id)
                .await
                .map_err(|_| {
                    "You are trying to update or delete a document which has been deleted"
                })?;

            document_id
        }
    };

    // Verify the claimed log id against the expected one for this document id and author.
    verify_log_id(store, &author, log_id, &document_id).await?;

    ///////////////
    // STORE LOG //
    ///////////////

    // If this is a CREATE operation it goes into a new log which we insert here.
    if operation.is_create() {
        let log = S::StorageLog::new(&author, &operation.schema(), &document_id, log_id);

        store.insert_log(log).await?;
    }

    /////////////////////////////////////
    // DETERMINE NEXT ENTRY ARG VALUES //
    /////////////////////////////////////

    // If we have reached MAX_SEQ_NUM here for the next args then we will error and _not_ store the
    // entry which is being processed in this request.
    let next_seq_num = increment_seq_num(&mut seq_num.clone()).map_err(|_| {
        anyhow!(
            "Max sequence number reached for {} log {}",
            author.display(),
            log_id.as_u64()
        )
    })?;
    let backlink = Some(entry_encoded.hash());

    // Check if skiplink is required and return hash if so
    let skiplink = if is_lipmaa_required(next_seq_num.as_u64()) {
        Some(get_expected_skiplink(store, &author, log_id, &next_seq_num).await?)
    } else {
        None
    }
    .map(|entry| entry.hash());

    let next_args = NextEntryArguments {
        log_id: (*log_id).into(),
        seq_num: next_seq_num.into(),
        backlink: backlink.map(|hash| hash.into()),
        skiplink: skiplink.map(|hash| hash.into()),
    };

    ///////////////////////////////
    // STORE ENTRY AND OPERATION //
    ///////////////////////////////

    // Insert the entry into the store.
    store
        .insert_entry(S::StorageEntry::new(entry_encoded, operation_encoded)?)
        .await?;
    // Insert the operation into the store.
    store
        .insert_operation(
            &S::StorageOperation::new(&author, &entry_encoded.hash().into(), &operation).unwrap(),
            &document_id,
        )
        .await?;

    Ok(next_args)
}

/// Attempt to identify the document id for view id contained in a `next_args` request.
///
/// This will fail if:
///
/// - any of the operations contained in the view id _don't_ exist in the store
/// - any of the operations contained in the view id return a different document id than any of the others
pub async fn get_checked_document_id_for_view_id<S: StorageProvider>(
    store: &S,
    view_id: &DocumentViewId,
) -> AnyhowResult<DocumentId> {
    let mut found_document_ids: HashSet<DocumentId> = HashSet::new();
    for operation in view_id.clone().into_iter() {
        // If any operation can't be found return an error at this point already.
        let document_id = store.get_document_by_operation_id(&operation).await?;

        ensure!(
            document_id.is_some(),
            anyhow!(
                "{} not found, could not determine document id",
                operation.display()
            )
        );

        found_document_ids.insert(document_id.unwrap());
    }

    // We can unwrap here as there must be at least one document view else the error above would
    // have been triggered.
    let mut found_document_ids_iter = found_document_ids.iter();
    let document_id = found_document_ids_iter.next().unwrap();

    ensure!(
        found_document_ids_iter.next().is_none(),
        anyhow!("Invalid document view id: operations in passed document view id originate from different documents")
    );
    Ok(document_id.to_owned())
}

#[cfg(test)]
mod tests {
    use std::convert::TryFrom;

    use crate::document::{DocumentId, DocumentViewId};
    use crate::entry::encode::sign_and_encode_entry;
    use crate::entry::{Entry, LogId, SeqNum};
    use crate::hash::Hash;
    use crate::identity::{Author, KeyPair};
    use crate::operation::{
        EncodedOperation, Operation, OperationFields, OperationId, OperationValue,
    };
    use crate::storage_provider::traits::{AsStorageEntry, EntryStore};
    use crate::test_utils::constants::{PRIVATE_KEY, SCHEMA_ID};
    use crate::test_utils::db::{MemoryStore, StorageEntry};
    use crate::test_utils::fixtures::{
        create_operation, delete_operation, key_pair, operation, operation_fields, public_key,
        random_document_view_id, random_hash, update_operation,
    };
    use rstest::rstest;

    use crate::db::stores::test_utils::{
        doggo_test_fields, encode_entry_and_operation, populate_test_db, send_to_store, test_db,
        test_db_config, PopulateDatabaseConfig, TestDatabase, TestDatabaseRunner,
    };
    use crate::domain::publish;
    use crate::graphql::client::NextEntryArguments;

    use super::{get_checked_document_id_for_view_id, next_args};

    type LogIdAndSeqNum = (u64, u64);

    /// Helper method for removing entries from a MemoryStore by Author & LogIdAndSeqNum.
    fn remove_entries(store: &MemoryStore, author: &Author, entries_to_remove: &[LogIdAndSeqNum]) {
        store.entries.lock().unwrap().retain(|_, entry| {
            !entries_to_remove.contains(&(entry.log_id().as_u64(), entry.seq_num().as_u64()))
                && &entry.author() == author
        });
    }

    /// Helper method for removing operations from a MemoryStore by Author & LogIdAndSeqNum.
    fn remove_operations(
        store: &MemoryStore,
        author: &Author,
        operations_to_remove: &[LogIdAndSeqNum],
    ) {
        for (hash, entry) in store.entries.lock().unwrap().iter() {
            if operations_to_remove.contains(&(entry.log_id().as_u64(), entry.seq_num().as_u64()))
                && &entry.author() == author
            {
                store
                    .operations
                    .lock()
                    .unwrap()
                    .remove(&hash.clone().into());
            }
        }
    }

    #[rstest]
    fn errors_when_passed_non_existent_view_id(
        #[from(test_db)] runner: TestDatabaseRunner,
        #[from(random_document_view_id)] document_view_id: DocumentViewId,
    ) {
        runner.with_db_teardown(|db: TestDatabase| async move {
            let result = get_checked_document_id_for_view_id(&db.store, &document_view_id).await;
            assert!(result.is_err());
        });
    }

    #[rstest]
    fn gets_document_id_for_view(
        #[from(test_db)] runner: TestDatabaseRunner,
        operation: Operation,
        operation_fields: OperationFields,
    ) {
        runner.with_db_teardown(|db: TestDatabase| async move {
            // Store one entry and operation in the store.
            let (entry, _) = send_to_store(&db.store, &operation, None, &KeyPair::new()).await;
            let operation_one_id: OperationId = entry.hash().into();

            // Store another entry and operation, from a different author, which perform an update on the earlier operation.
            let update_operation = Operation::new_update(
                SCHEMA_ID.parse().unwrap(),
                operation_one_id.clone().into(),
                operation_fields,
            )
            .unwrap();

            let (entry, _) = send_to_store(
                &db.store,
                &update_operation,
                Some(&entry.hash().into()),
                &KeyPair::new(),
            )
            .await;
            let operation_two_id: OperationId = entry.hash().into();

            // Get the document id for the passed view id.
            let result = get_checked_document_id_for_view_id(
                &db.store,
                &DocumentViewId::new(&[operation_one_id.clone(), operation_two_id]).unwrap(),
            )
            .await;

            // Result should be ok.
            assert!(result.is_ok());

            // The returned document id should match the expected one.
            let document_id = result.unwrap();
            assert_eq!(document_id, DocumentId::new(operation_one_id))
        });
    }

    #[rstest]
    #[case::ok(&[(0, 8)], (0, 8))]
    #[should_panic(
        expected = "Expected skiplink target not found in store: <Author 53fc96>, log id 0, seq num 4"
    )]
    #[case::skiplink_missing(&[(0, 4), (0, 8)], (0, 8))]
    #[should_panic(
        expected = "Entry's claimed seq num of 8 does not match expected seq num of 7 for given author and log"
    )]
    #[case::backlink_missing(&[(0, 7), (0, 8)], (0, 8))]
    #[should_panic(
        expected = "Entry's claimed seq num of 8 does not match expected seq num of 7 for given author and log"
    )]
    #[case::backlink_and_skiplink_missing(&[(0, 4), (0, 7), (0, 8)], (0, 8))]
    #[should_panic(
        expected = "Entry's claimed seq num of 8 does not match expected seq num of 9 for given author and log"
    )]
    #[case::seq_num_occupied_again(&[], (0, 8))]
    #[should_panic(
        expected = "Entry's claimed seq num of 7 does not match expected seq num of 9 for given author and log"
    )]
    #[case::seq_num_occupied_(&[], (0, 7))]
    #[should_panic(
        expected = "Expected skiplink target not found in store: <Author 53fc96>, log id 0, seq num 4"
    )]
    #[case::next_args_skiplink_missing(&[(0, 4), (0, 7), (0, 8)], (0, 7))]
    #[should_panic(
        expected = "Entry's claimed seq num of 8 does not match expected seq num of 1 for given author and log"
    )]
    #[case::no_entries_yet(&[(0, 1), (0, 2), (0, 3), (0, 4), (0, 5), (0, 6), (0, 7), (0, 8)], (0, 8))]
    #[tokio::test]
    async fn publish_with_missing_entries(
        #[case] entries_to_remove: &[LogIdAndSeqNum],
        #[case] entry_to_publish: LogIdAndSeqNum,
        #[from(test_db_config)]
        #[with(8, 1, 1)]
        config: PopulateDatabaseConfig,
    ) {
        let store = MemoryStore::default();
        let mut db = TestDatabase::new(store.clone());
        populate_test_db(&mut db, &config).await;

        // The author who has published to the db.
        let author = Author::try_from(db.test_data.key_pairs[0].public_key().to_owned()).unwrap();

        // Get the latest entry from the db.
        let next_entry = db
            .store
            .get_entry_at_seq_num(
                &author,
                &LogId::new(entry_to_publish.0),
                &SeqNum::new(entry_to_publish.1).unwrap(),
            )
            .await
            .unwrap()
            .unwrap();

        // Remove some entries and operations from the database.
        remove_operations(&db.store, &author, entries_to_remove);
        remove_entries(&db.store, &author, entries_to_remove);

        // Publish the latest entry again and see what happens.
        let result = publish(
            &db.store,
            &next_entry.entry_signed(),
            &next_entry.operation_encoded().unwrap(),
        )
        .await;

        // Unwrap here causing a panic, we check the errors match what we expect.
        result.unwrap();
    }

    #[rstest]
    #[case::ok_single_writer(&[], &[(0, 8)], KeyPair::from_private_key_str(PRIVATE_KEY).unwrap())]
    // Weird case where all previous operations are on the same branch, but still valid.
    #[case::ok_many_previous_operations(&[], &[(0, 8), (0, 7), (0, 6)], KeyPair::from_private_key_str(PRIVATE_KEY).unwrap())]
    #[case::ok_multi_writer(&[], &[(0, 8)], KeyPair::new())]
    #[should_panic(expected = "<Operation 76e89a> not found, could not determine document id")]
    #[case::previous_operation_missing(&[(0, 8)], &[(0, 8)], KeyPair::from_private_key_str(PRIVATE_KEY).unwrap())]
    #[should_panic(expected = "<Operation 51fbba> not found, could not determine document id")]
    #[case::one_of_some_previous_operations_missing(&[(0, 7)], &[(0, 7), (0, 8)], KeyPair::from_private_key_str(PRIVATE_KEY).unwrap())]
    #[should_panic(expected = "<Operation 76e89a> not found, could not determine document id")]
    #[case::one_of_some_previous_operations_missing(&[(0, 8)], &[(0, 7), (0, 8)], KeyPair::from_private_key_str(PRIVATE_KEY).unwrap())]
    #[should_panic(expected = "<Operation 76e89a> not found, could not determine document id")]
    #[case::missing_previous_operation_multi_writer(&[(0, 8)], &[(0, 8)], KeyPair::new())]
    #[should_panic(
        expected = "Invalid document view id: operations in passed document view id originate from different documents"
    )]
    #[case::previous_operations_invalid_multiple_document_id(&[], &[(0, 8), (1, 8)], KeyPair::from_private_key_str(PRIVATE_KEY).unwrap())]
    #[tokio::test]
    async fn publish_with_missing_operations(
        // The operations to be removed from the db
        #[case] operations_to_remove: &[LogIdAndSeqNum],
        // The previous operations described by their log id and seq number (log_id, seq_num)
        #[case] previous_operations: &[LogIdAndSeqNum],
        #[case] key_pair: KeyPair,
        #[from(test_db_config)]
        #[with(8, 2, 1)]
        config: PopulateDatabaseConfig,
    ) {
        let store = MemoryStore::default();
        let mut db = TestDatabase::new(store.clone());
        populate_test_db(&mut db, &config).await;

        let author = Author::try_from(db.test_data.key_pairs[0].public_key().to_owned()).unwrap();

        // Get the document id.
        let document_id = db.test_data.documents.first().unwrap();

        // Map the passed &[LogIdAndSeqNum] into a DocumentViewId containing the claimed operations.
        let previous_operations: Vec<OperationId> = previous_operations
            .iter()
            .filter_map(|(log_id, seq_num)| {
                db.store
                    .entries
                    .lock()
                    .unwrap()
                    .values()
                    .find(|entry| {
                        entry.seq_num().as_u64() == *seq_num
                            && entry.log_id.as_u64() == *log_id
                            && entry.author() == author
                    })
                    .map(|entry| entry.hash().into())
            })
            .collect();
        // Construct document view id for previous operations.
        let document_view_id = DocumentViewId::new(&previous_operations).unwrap();

        // Compose the next operation.
        let next_operation = Operation::new_update(
            SCHEMA_ID.parse().unwrap(),
            document_view_id,
            operation_fields(doggo_test_fields()),
        )
        .unwrap();

        // Encode an entry and the operation.
        let (entry, operation) =
            encode_entry_and_operation(&db.store, &next_operation, &key_pair, Some(document_id))
                .await;

        // Remove some entries from the db.
        remove_operations(&db.store, &author, operations_to_remove);

        // Publish the entry and operation.
        let result = publish(&db.store, &entry, &operation).await;

        // Unwrap here causing a panic, we check the errors match what we expect.
        result.unwrap();
    }

    #[rstest]
    #[case::ok_single_writer(&[], &[(0, 8)], KeyPair::from_private_key_str(PRIVATE_KEY).unwrap())]
    #[case::ok_many_previous_operations(&[], &[(0, 8), (0, 7), (0, 6)], KeyPair::from_private_key_str(PRIVATE_KEY).unwrap())]
    #[case::ok_not_the_most_recent_document_view_id(&[], &[(0, 1)], KeyPair::from_private_key_str(PRIVATE_KEY).unwrap())]
    #[case::ok_multi_writer(&[], &[(0, 8)], KeyPair::new())]
    #[should_panic(expected = "<Operation 76e89a> not found, could not determine document id")]
    #[case::previous_operation_missing(&[(0, 8)], &[(0, 8)], KeyPair::from_private_key_str(PRIVATE_KEY).unwrap())]
    #[should_panic(expected = "<Operation 51fbba> not found, could not determine document id")]
    #[case::one_of_some_previous_operations_missing(&[(0, 7)], &[(0, 7), (0, 8)], KeyPair::from_private_key_str(PRIVATE_KEY).unwrap())]
    #[should_panic(expected = "<Operation 76e89a> not found, could not determine document id")]
    #[case::one_of_some_previous_operations_missing(&[(0, 8)], &[(0, 7), (0, 8)], KeyPair::from_private_key_str(PRIVATE_KEY).unwrap())]
    #[should_panic(expected = "<Operation 76e89a> not found, could not determine document id")]
    #[case::missing_previous_operation_multi_writer(&[(0, 8)], &[(0, 8)], KeyPair::new())]
    #[should_panic(
        expected = "Invalid document view id: operations in passed document view id originate from different documents"
    )]
    #[case::previous_operations_invalid_multiple_document_id(&[], &[(0, 8), (1, 8)], KeyPair::from_private_key_str(PRIVATE_KEY).unwrap())]
    #[tokio::test]
    async fn next_args_with_missing_operations(
        #[case] operations_to_remove: &[LogIdAndSeqNum],
        #[case] document_view_id: &[LogIdAndSeqNum],
        #[case] key_pair: KeyPair,
        #[from(test_db_config)]
        #[with(8, 2, 1)]
        config: PopulateDatabaseConfig,
    ) {
        let store = MemoryStore::default();
        let mut db = TestDatabase::new(store.clone());
        populate_test_db(&mut db, &config).await;

        let author_with_removed_operations =
            Author::try_from(db.test_data.key_pairs[0].public_key().to_owned()).unwrap();
        let author_making_request = Author::try_from(key_pair.public_key().to_owned()).unwrap();

        // Map the passed &[LogIdAndSeqNum] into a DocumentViewId containing the claimed operations.
        let document_view_id: Vec<OperationId> = document_view_id
            .iter()
            .filter_map(|(log_id, seq_num)| {
                db.store
                    .entries
                    .lock()
                    .unwrap()
                    .values()
                    .find(|entry| {
                        entry.seq_num().as_u64() == *seq_num
                            && entry.log_id.as_u64() == *log_id
                            && entry.author() == author_with_removed_operations
                    })
                    .map(|entry| entry.hash().into())
            })
            .collect();

        // Construct document view id for previous operations.
        let document_view_id = DocumentViewId::new(&document_view_id).unwrap();

        // Remove some operations.
        remove_operations(
            &db.store,
            &author_with_removed_operations,
            operations_to_remove,
        );

        // Get the next args.
        let result = next_args(&db.store, &author_making_request, Some(&document_view_id)).await;

        // Unwrap here causing a panic, we check the errors match what we expect.
        result.unwrap();
    }

    type SeqNumU64 = u64;
    type Backlink = Option<u64>;
    type Skiplink = Option<u64>;

    #[rstest]
    #[case(0, None, (1, None, None))]
    #[case(1, Some(1), (2, Some(1), None))]
    #[case(2, Some(2), (3, Some(2), None))]
    #[case(3, Some(3), (4, Some(3), Some(1)))]
    #[case(4, Some(4), (5, Some(4), None))]
    #[case(5, Some(5), (6, Some(5), None))]
    #[case(6, Some(6), (7, Some(6), None))]
    #[case(7, Some(7), (8, Some(7), Some(4)))]
    #[case(2, Some(1), (3, Some(2), None))]
    #[case(3, Some(1), (4, Some(3), Some(1)))]
    #[case(4, Some(1), (5, Some(4), None))]
    #[case(5, Some(1), (6, Some(5), None))]
    #[case(6, Some(1), (7, Some(6), None))]
    #[case(7, Some(1), (8, Some(7), Some(4)))]
    #[tokio::test]
    async fn next_args_with_expected_results(
        #[case] no_of_entries: usize,
        #[case] document_view_id: Option<SeqNumU64>,
        #[case] expected_next_args: (SeqNumU64, Backlink, Skiplink),
    ) {
        let store = MemoryStore::default();
        let mut db = TestDatabase::new(store.clone());
        // Populate the db with the number of entries defined in the test params.
        let config = PopulateDatabaseConfig {
            no_of_entries,
            no_of_logs: 1,
            no_of_authors: 1,
            ..PopulateDatabaseConfig::default()
        };
        populate_test_db(&mut db, &config).await;

        // The author who published the entries.
        let author = Author::try_from(db.test_data.key_pairs[0].public_key().to_owned()).unwrap();

        // Construct the passed document view id (specified by a single sequence number)
        let document_view_id: Option<DocumentViewId> = document_view_id.map(|seq_num| {
            db.store
                .entries
                .lock()
                .unwrap()
                .values()
                .find(|entry| entry.seq_num().as_u64() == seq_num)
                .map(|entry| DocumentViewId::new(&[entry.hash().into()]).unwrap())
                .unwrap()
        });

        // Construct the expected next args
        let expected_seq_num = SeqNum::new(expected_next_args.0).unwrap();
        let expected_log_id = LogId::default();
        let expected_backlink = match expected_next_args.1 {
            Some(backlink) => db
                .store
                .get_entry_at_seq_num(&author, &expected_log_id, &SeqNum::new(backlink).unwrap())
                .await
                .unwrap()
                .map(|entry| entry.hash()),
            None => None,
        };
        let expected_skiplink = match expected_next_args.2 {
            Some(skiplink) => db
                .store
                .get_entry_at_seq_num(&author, &expected_log_id, &SeqNum::new(skiplink).unwrap())
                .await
                .unwrap()
                .map(|entry| entry.hash()),
            None => None,
        };
        let expected_next_args = NextEntryArguments {
            log_id: expected_log_id.into(),
            seq_num: expected_seq_num.into(),
            backlink: expected_backlink.map(|hash| hash.into()),
            skiplink: expected_skiplink.map(|hash| hash.into()),
        };

        // Request next args for the author and docuent view.
        let result = next_args(&db.store, &author, document_view_id.as_ref()).await;
        assert_eq!(result.unwrap(), expected_next_args);
    }

    #[rstest]
    #[tokio::test]
    async fn gets_next_args_other_cases(
        public_key: Author,
        #[from(test_db_config)]
        #[with(7, 1, 1)]
        config: PopulateDatabaseConfig,
    ) {
        let store = MemoryStore::default();
        let mut db = TestDatabase::new(store.clone());
        populate_test_db(&mut db, &config).await;

        // Get with no DocumentViewId given.
        let result = next_args(&db.store, &public_key, None).await;
        assert!(result.is_ok());
        assert_eq!(
            NextEntryArguments {
                backlink: None,
                skiplink: None,
                log_id: LogId::new(1).into(),
                seq_num: SeqNum::default().into(),
            },
            result.unwrap()
        );

        // Get with non-existent DocumentViewId given.
        let result = next_args(&db.store, &public_key, Some(&random_document_view_id())).await;
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .message
                .as_str()
                .contains("could not determine document id") // This is a partial string match, preceded by "<Operation xxxxx> not found,"
        );

        // Here we are missing the skiplink.
        remove_entries(&db.store, &public_key, &[(0, 4)]);
        let document_id = db.test_data.documents.get(0).unwrap();
        let document_view_id =
            DocumentViewId::new(&[document_id.as_str().parse().unwrap()]).unwrap();

        let result = next_args(&db.store, &public_key, Some(&document_view_id)).await;
        assert_eq!(
            result.unwrap_err().message.as_str(),
            "Expected skiplink target not found in store: <Author 53fc96>, log id 0, seq num 4"
        );
    }

    #[rstest]
    #[case::owner_publishes_update_to_correct_log(LogId::new(0), KeyPair::from_private_key_str(PRIVATE_KEY).unwrap())]
    #[case::new_author_updates_to_new_log(LogId::new(0), KeyPair::new())]
    #[should_panic(
        expected = "Entry's claimed log id of 1 does not match existing log id of 0 for given author and document"
    )]
    #[case::owner_updates_to_wrong_and_taken_log(LogId::new(1), KeyPair::from_private_key_str(PRIVATE_KEY).unwrap())]
    #[should_panic(
        expected = "Entry's claimed log id of 2 does not match existing log id of 0 for given author and document"
    )]
    #[case::owner_updates_to_wrong_but_free_log(LogId::new(2), KeyPair::from_private_key_str(PRIVATE_KEY).unwrap())]
    #[should_panic(
        expected = "Entry's claimed log id of 1 does not match expected next log id of 0 for given author"
    )]
    #[case::new_author_updates_to_wrong_new_log(LogId::new(1), KeyPair::new())]
    #[tokio::test]
    async fn publish_update_log_tests(
        #[case] log_id: LogId,
        #[case] key_pair: KeyPair,
        #[from(test_db_config)]
        #[with(2, 1, 1)]
        config: PopulateDatabaseConfig,
    ) {
        let store = MemoryStore::default();
        let mut db = TestDatabase::new(store.clone());
        populate_test_db(&mut db, &config).await;

        let document_id = db.test_data.documents.first().unwrap();
        let document_view_id: DocumentViewId = document_id.as_str().parse().unwrap();
        let author_performing_update = Author::try_from(key_pair.public_key().to_owned()).unwrap();

        let update_operation = Operation::new_update(
            SCHEMA_ID.parse().unwrap(),
            document_view_id.clone(),
            operation_fields(doggo_test_fields()),
        )
        .unwrap();

        let latest_entry = db
            .store
            .get_latest_entry(&author_performing_update, &log_id)
            .await
            .unwrap();

        let entry = Entry::new(
            &log_id,
            Some(&update_operation),
            None,
            latest_entry.as_ref().map(|entry| entry.hash()).as_ref(),
            &latest_entry
                .map(|entry| entry.seq_num().next().unwrap())
                .unwrap_or_default(),
        )
        .unwrap();

        let entry_encoded = sign_and_encode_entry(&entry, &key_pair).unwrap();
        let operation_encoded = EncodedOperation::try_from(&update_operation).unwrap();

        let result = publish(&db.store, &entry_encoded, &operation_encoded).await;

        result.unwrap();
    }

    #[rstest]
    #[case::owner_publishes_to_correct_log(LogId::new(2), KeyPair::from_private_key_str(PRIVATE_KEY).unwrap())]
    #[case::new_author_publishes_to_new_log(LogId::new(0), KeyPair::new())]
    #[should_panic(
        expected = "Entry's claimed seq num of 1 does not match expected seq num of 2 for given author and log"
    )]
    #[case::owner_publishes_to_wrong_and_taken_log(LogId::new(1), KeyPair::from_private_key_str(PRIVATE_KEY).unwrap())]
    #[should_panic(
        expected = "Entry's claimed log id of 3 does not match expected next log id of 2 for given author"
    )]
    #[case::owner_publishes_to_wrong_but_free_log(LogId::new(3), KeyPair::from_private_key_str(PRIVATE_KEY).unwrap())]
    #[should_panic(
        expected = "Entry's claimed log id of 1 does not match expected next log id of 0 for given author"
    )]
    #[case::new_author_publishes_to_wrong_new_log(LogId::new(1), KeyPair::new())]
    #[tokio::test]
    async fn publish_create_log_tests(
        #[case] log_id: LogId,
        #[case] key_pair: KeyPair,
        operation: Operation,
        #[from(test_db_config)]
        #[with(1, 2, 1)]
        config: PopulateDatabaseConfig,
    ) {
        let store = MemoryStore::default();
        let mut db = TestDatabase::new(store.clone());
        populate_test_db(&mut db, &config).await;

        let entry = Entry::new(&log_id, Some(&operation), None, None, &SeqNum::default()).unwrap();

        let entry_encoded = sign_and_encode_entry(&entry, &key_pair).unwrap();
        let operation_encoded = EncodedOperation::try_from(&operation).unwrap();

        let result = publish(&db.store, &entry_encoded, &operation_encoded).await;

        result.unwrap();
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
        #[case] key_pair: KeyPair,
        #[from(test_db_config)]
        #[with(2, 1, 1, true)]
        config: PopulateDatabaseConfig,
    ) {
        let store = MemoryStore::default();
        let mut db = TestDatabase::new(store.clone());
        populate_test_db(&mut db, &config).await;

        let document_id = db.test_data.documents.first().unwrap();
        let document_view_id: DocumentViewId = document_id.as_str().parse().unwrap();
        let author_performing_update = Author::try_from(key_pair.public_key().to_owned()).unwrap();

        let delete_operation =
            Operation::new_delete(SCHEMA_ID.parse().unwrap(), document_view_id.clone()).unwrap();

        let latest_entry = db
            .store
            .get_latest_entry(&author_performing_update, &LogId::default())
            .await
            .unwrap();

        let entry = Entry::new(
            &LogId::default(),
            Some(&delete_operation),
            None,
            latest_entry.as_ref().map(|entry| entry.hash()).as_ref(),
            &latest_entry
                .map(|entry| entry.seq_num().next().unwrap())
                .unwrap_or_default(),
        )
        .unwrap();

        let entry_encoded = sign_and_encode_entry(&entry, &key_pair).unwrap();
        let operation_encoded = EncodedOperation::try_from(&delete_operation).unwrap();

        let result = publish(&db.store, &entry_encoded, &operation_encoded).await;

        result.unwrap();
    }

    #[rstest]
    #[should_panic(expected = "Document is deleted")]
    #[case(KeyPair::from_private_key_str(PRIVATE_KEY).unwrap())]
    #[should_panic(expected = "Document is deleted")]
    #[case(KeyPair::new())]
    #[tokio::test]
    async fn next_args_deleted_documents(
        #[case] key_pair: KeyPair,
        #[from(test_db_config)]
        #[with(3, 1, 1, true)]
        config: PopulateDatabaseConfig,
    ) {
        let store = MemoryStore::default();
        let mut db = TestDatabase::new(store.clone());
        populate_test_db(&mut db, &config).await;

        let document_id = db.test_data.documents.first().unwrap();
        let document_view_id: DocumentViewId = document_id.as_str().parse().unwrap();
        let author = Author::try_from(key_pair.public_key().to_owned()).unwrap();

        let result = next_args(&db.store, &author, Some(&document_view_id)).await;

        result.unwrap();
    }

    #[rstest]
    fn publish_many_entries(key_pair: KeyPair, #[from(test_db)] runner: TestDatabaseRunner) {
        runner.with_db_teardown(|db: TestDatabase| async move {
            let num_of_entries = 13;
            let mut document_id: Option<DocumentId> = None;
            let author = Author::try_from(key_pair.public_key().to_owned()).unwrap();
            for index in 0..num_of_entries {
                let document_view_id: Option<DocumentViewId> =
                    document_id.clone().map(|id| id.as_str().parse().unwrap());

                let next_entry_args = next_args(&db.store, &author, document_view_id.as_ref())
                    .await
                    .unwrap();

                let operation = if index == 0 {
                    create_operation(&[("name", OperationValue::Text("Panda".to_string()))])
                } else if index == (num_of_entries - 1) {
                    delete_operation(&next_entry_args.backlink.clone().unwrap().into())
                } else {
                    update_operation(
                        &[("name", OperationValue::Text("🐼".to_string()))],
                        &next_entry_args.backlink.clone().unwrap().into(),
                    )
                };

                let entry = Entry::new(
                    &next_entry_args.log_id.into(),
                    Some(&operation),
                    next_entry_args.skiplink.map(Hash::from).as_ref(),
                    next_entry_args.backlink.map(Hash::from).as_ref(),
                    &next_entry_args.seq_num.into(),
                )
                .unwrap();

                let entry_encoded = sign_and_encode_entry(&entry, &key_pair).unwrap();
                let operation_encoded = EncodedOperation::try_from(&operation).unwrap();

                if index == 0 {
                    document_id = Some(entry_encoded.hash().into());
                }

                let result = publish(&db.store, &entry_encoded, &operation_encoded).await;

                assert!(result.is_ok());
            }
        });
    }

    #[rstest]
    #[should_panic(expected = "Max sequence number reached for <Author 53fc96> log 0")]
    #[tokio::test]
    async fn next_args_max_seq_num_reached(
        key_pair: KeyPair,
        #[from(test_db_config)]
        #[with(2, 1, 1, false)]
        config: PopulateDatabaseConfig,
    ) {
        let store = MemoryStore::default();
        let mut db = TestDatabase::new(store.clone());
        populate_test_db(&mut db, &config).await;

        let author = Author::try_from(key_pair.public_key().to_owned()).unwrap();

        let entry_two = db
            .store
            .get_entry_at_seq_num(&author, &LogId::default(), &SeqNum::new(2).unwrap())
            .await
            .unwrap()
            .unwrap();

        let entry = Entry::new(
            &LogId::default(),
            Some(&entry_two.operation()),
            Some(&random_hash()),
            Some(&random_hash()),
            &SeqNum::new(u64::MAX).unwrap(),
        )
        .unwrap();

        let entry_encoded = sign_and_encode_entry(&entry, &key_pair).unwrap();

        let entry =
            StorageEntry::new(&entry_encoded, &entry_two.operation_encoded().unwrap()).unwrap();

        db.store
            .entries
            .lock()
            .unwrap()
            .insert(entry.hash(), entry.clone());

        let result = next_args(&db.store, &author, Some(&entry_two.hash().into())).await;

        result.unwrap();
    }

    #[rstest]
    #[should_panic(expected = "Max sequence number reached for <Author 53fc96> log 0")]
    #[tokio::test]
    async fn publish_max_seq_num_reached(
        key_pair: KeyPair,
        #[from(test_db_config)]
        #[with(2, 1, 1, false)]
        config: PopulateDatabaseConfig,
    ) {
        let store = MemoryStore::default();
        let mut db = TestDatabase::new(store.clone());
        populate_test_db(&mut db, &config).await;

        let author = Author::try_from(key_pair.public_key().to_owned()).unwrap();

        // Get the latest entry, we will use it's operation in all other entries (doesn't matter if it's a duplicate, just need the previous
        // operations to exist).
        let entry_two = db
            .store
            .get_entry_at_seq_num(&author, &LogId::default(), &SeqNum::new(2).unwrap())
            .await
            .unwrap()
            .unwrap();

        // Create and insert the skiplink for MAX_SEQ_NUM entry
        let skiplink = Entry::new(
            &LogId::default(),
            Some(&entry_two.operation()),
            Some(&random_hash()),
            Some(&random_hash()),
            &SeqNum::new(18446744073709551611).unwrap(),
        )
        .unwrap();

        let entry_encoded = sign_and_encode_entry(&skiplink, &key_pair).unwrap();

        let skiplink =
            StorageEntry::new(&entry_encoded, &entry_two.operation_encoded().unwrap()).unwrap();

        db.store
            .entries
            .lock()
            .unwrap()
            .insert(skiplink.hash(), skiplink.clone());

        // Create and insert the backlink for MAX_SEQ_NUM entry
        let backlink = Entry::new(
            &LogId::default(),
            Some(&entry_two.operation()),
            Some(&random_hash()),
            Some(&random_hash()),
            &SeqNum::new(u64::MAX - 1).unwrap(),
        )
        .unwrap();

        let entry_encoded = sign_and_encode_entry(&backlink, &key_pair).unwrap();

        let backlink =
            StorageEntry::new(&entry_encoded, &entry_two.operation_encoded().unwrap()).unwrap();

        db.store
            .entries
            .lock()
            .unwrap()
            .insert(backlink.hash(), backlink.clone());

        // Create the MAX_SEQ_NUM entry using the above skiplink and backlink
        let entry_with_max_seq_num = Entry::new(
            &LogId::default(),
            Some(&entry_two.operation()),
            Some(&skiplink.hash()),
            Some(&backlink.hash()),
            &SeqNum::new(u64::MAX).unwrap(),
        )
        .unwrap();

        let entry_encoded = sign_and_encode_entry(&entry_with_max_seq_num, &key_pair).unwrap();

        // Publish the MAX_SEQ_NUM entry
        let result = publish(
            &db.store,
            &entry_encoded,
            &entry_two.operation_encoded().unwrap(),
        )
        .await;

        // try and get the MAX_SEQ_NUM entry again (it shouldn't be there)
        let entry_at_max_seq_num = db
            .store
            .get_entry_by_hash(&entry_encoded.hash())
            .await
            .unwrap();

        // We expect the entry we published not to have been stored in the db
        assert!(entry_at_max_seq_num.is_none());
        result.unwrap();
    }
}
