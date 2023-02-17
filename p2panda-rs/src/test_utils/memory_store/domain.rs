// SPDX-License-Identifier: AGPL-3.0-or-later

//! Methods to validate data coming through the p2panda API.
//!
//! Currently these implementations are used for testing purposes but might be factored out into
//! actual, reusable components of this crate.
use bamboo_rs_core_ed25519_yasmf::entry::is_lipmaa_required;

use crate::document::DocumentViewId;
use crate::entry::decode::decode_entry;
use crate::entry::traits::{AsEncodedEntry, AsEntry};
use crate::entry::{EncodedEntry, LogId, SeqNum};
use crate::hash::Hash;
use crate::identity::PublicKey;
use crate::operation::plain::PlainOperation;
use crate::operation::traits::AsOperation;
use crate::operation::validate::validate_operation_with_entry;
use crate::operation::{EncodedOperation, OperationAction};
use crate::schema::Schema;
use crate::storage_provider::traits::{EntryStore, LogStore, OperationStore};
use crate::test_utils::memory_store::errors::DomainError;
use crate::test_utils::memory_store::validation::{
    ensure_document_not_deleted, get_checked_document_id_for_view_id, get_expected_skiplink,
    increment_seq_num, is_next_seq_num, next_log_id, verify_log_id,
};

/// An entries' backlink returned by next_args.
type Backlink = Hash;

/// An entries' skiplink returned by next_args.
type Skiplink = Hash;

/// Retrieve arguments required for constructing the next entry in a bamboo log for a specific
/// public key and document.
///
/// We accept a `DocumentViewId` rather than a `DocumentId` as an argument and then identify the
/// document id based on operations already existing in the store. Doing this means a document can
/// be updated without knowing the document id itself.
///
/// This method is intended to be used behind a public API and so we assume all passed values are
/// in themselves valid.
///
/// The steps and validation checks this method performs are:
///
/// Check if a document view id was passed
///
/// - if it wasn't, we are creating a new document, safely increment the latest log id for the
/// passed public key and return args immediately
/// - if it was, continue knowing we are updating an existing document
///
/// Determine the document id we are concerned with
///
/// - verify that all operations in the passed document view id exist in the database
/// - verify that all operations in the passed document view id are from the same document
/// - ensure the document is not deleted
///
/// Determine next arguments
///
/// - get the log id for this public key and document id, or if none is found safely increment this
/// public keys latest log id
/// - get the backlink entry (latest entry for this public key and log)
/// - get the skiplink for this public key, log and next seq num
/// - get the latest seq num for this public key and log and safely increment
///
/// Finally, return next arguments.
pub async fn next_args<S: EntryStore + OperationStore + LogStore>(
    store: &S,
    public_key: &PublicKey,
    document_view_id: Option<&DocumentViewId>,
) -> Result<(Option<Backlink>, Option<Skiplink>, SeqNum, LogId), DomainError> {
    ////////////////////////
    // HANDLE CREATE CASE //
    ////////////////////////

    // If no document_view_id is passed then this is a request for publishing a CREATE operation
    // and we return the args for the next free log by this public_key.
    if document_view_id.is_none() {
        let log_id = next_log_id(store, public_key).await?;
        return Ok((None, None, SeqNum::default(), log_id));
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

    // Retrieve the log_id for the found document_id and public_key.
    let log_id = store.get_log_id(public_key, &document_id).await?;

    // Check if an existing log id was found for this public key and document.
    match log_id {
        // If it wasn't found, we just calculate the next log id safely and return the next args.
        None => {
            let next_log_id = next_log_id(store, public_key).await?;
            Ok((None, None, SeqNum::default(), next_log_id))
        }
        // If one was found, we need to get the backlink and skiplink, and safely increment the seq
        // num.
        Some(log_id) => {
            // Get the latest entry in this log.
            let latest_entry = store.get_latest_entry(public_key, &log_id).await?;

            // Determine the next sequence number by incrementing one from the latest entry seq
            // num.
            //
            // If the latest entry is None, then we must be at seq num 1.
            let seq_num = match latest_entry {
                Some(ref latest_entry) => {
                    let mut latest_seq_num = latest_entry.seq_num().to_owned();
                    increment_seq_num(&mut latest_seq_num).map_err(|_| {
                        DomainError::MaxSeqNumReached(public_key.to_string(), log_id.as_u64())
                    })
                }
                None => Ok(SeqNum::default()),
            }?;

            // Check if skiplink is required and if it is get the entry and return its hash.
            let skiplink = if is_lipmaa_required(seq_num.as_u64()) {
                // Determine skiplink ("lipmaa"-link) entry in this log.
                Some(get_expected_skiplink(store, public_key, &log_id, &seq_num).await?)
            } else {
                None
            }
            .map(|entry| entry.hash());

            let backlink = latest_entry.map(|entry| entry.hash().into());
            let skiplink = skiplink.map(|hash| hash.into());

            Ok((backlink, skiplink, seq_num, log_id))
        }
    }
}

/// Persist an entry and operation to storage after performing validation of claimed values against
/// expected values retrieved from storage.
///
/// Returns the arguments required for constructing the next entry in a bamboo log for the
/// specified public key and document.
///
/// This method is intended to be used behind a public API and so we assume all passed values are
/// in themselves valid.
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
    //////////////////
    // DECODE ENTRY //
    //////////////////

    let entry = decode_entry(encoded_entry)?;
    let public_key = entry.public_key();
    let log_id = entry.log_id();
    let seq_num = entry.seq_num();

    //////////////////////////////////
    // VALIDATE ENTRY AND OPERATION //
    //////////////////////////////////

    // Verify that the claimed seq num matches the expected seq num for this public_key and log.
    let latest_entry = store.get_latest_entry(public_key, log_id).await?;
    let latest_seq_num = latest_entry.as_ref().map(|entry| entry.seq_num());
    is_next_seq_num(latest_seq_num, seq_num)?;

    // The backlink for this entry is the latest entry from this public key's log.
    let backlink = latest_entry;

    // If a skiplink is claimed, get the expected skiplink from the database, errors
    // if it can't be found.
    let skiplink = match entry.skiplink() {
        Some(_) => Some(get_expected_skiplink(store, public_key, log_id, seq_num).await?),
        None => None,
    };

    let skiplink_params = skiplink.map(|entry| {
        let hash = entry.hash();
        (entry, hash)
    });

    let backlink_params = backlink.map(|entry| {
        let hash = entry.hash();
        (entry, hash)
    });

    // Perform validation of the entry and it's operation.
    let (operation, operation_id) = validate_operation_with_entry(
        &entry,
        encoded_entry,
        skiplink_params.as_ref().map(|(entry, hash)| (entry, hash)),
        backlink_params.as_ref().map(|(entry, hash)| (entry, hash)),
        plain_operation,
        encoded_operation,
        schema,
    )?;

    ///////////////////////////
    // DETERMINE DOCUMENT ID //
    ///////////////////////////

    let document_id = match operation.action() {
        OperationAction::Create => {
            // Derive the document id for this new document.
            encoded_entry.hash().into()
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

            document_id
        }
    };

    // Verify the claimed log id against the expected one for this document id and public_key.
    verify_log_id(store, public_key, log_id, &document_id).await?;

    /////////////////////////////////////
    // DETERMINE NEXT ENTRY ARG VALUES //
    /////////////////////////////////////

    // If we have reached MAX_SEQ_NUM here for the next args then we will error and _not_ store the
    // entry which is being processed in this request.
    let next_seq_num = increment_seq_num(&mut seq_num.clone())
        .map_err(|_| DomainError::MaxSeqNumReached(public_key.to_string(), log_id.as_u64()))?;

    let backlink = Some(encoded_entry.hash());

    // Check if skiplink is required and return hash if so
    let skiplink = if is_lipmaa_required(next_seq_num.as_u64()) {
        Some(get_expected_skiplink(store, public_key, log_id, &next_seq_num).await?)
    } else {
        None
    }
    .map(|entry| entry.hash());

    ///////////////
    // STORE LOG //
    ///////////////

    // If the entries' seq num is 1 we insert a new log here.
    if entry.seq_num().is_first() {
        store
            .insert_log(log_id, public_key, &operation.schema_id(), &document_id)
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
        .insert_operation(&operation_id, public_key, &operation, &document_id)
        .await?;

    Ok((backlink, skiplink, next_seq_num, log_id.to_owned()))
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use crate::document::{DocumentId, DocumentViewId};
    use crate::entry::encode::sign_and_encode_entry;
    use crate::entry::traits::{AsEncodedEntry, AsEntry};
    use crate::entry::{LogId, SeqNum};
    use crate::hash::Hash;
    use crate::identity::{KeyPair, PublicKey};
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
        create_operation, delete_operation, key_pair, operation, random_document_view_id,
        random_hash, schema, update_operation,
    };
    use crate::test_utils::memory_store::helpers::{
        populate_store, send_to_store, PopulateStoreConfig,
    };
    use crate::test_utils::memory_store::{MemoryStore, StorageEntry};

    use super::{get_checked_document_id_for_view_id, next_args, publish};

    type LogIdAndSeqNum = (u64, u64);

    /// Helper method for removing entries from a MemoryStore by PublicKey & LogIdAndSeqNum.
    fn remove_entries(
        store: &MemoryStore,
        public_key: &PublicKey,
        entries_to_remove: &[LogIdAndSeqNum],
    ) {
        store.entries.lock().unwrap().retain(|_, entry| {
            !entries_to_remove.contains(&(entry.log_id().as_u64(), entry.seq_num().as_u64()))
                && entry.public_key() == public_key
        });
    }

    /// Helper method for removing operations from a MemoryStore by PublicKey & LogIdAndSeqNum.
    fn remove_operations(
        store: &MemoryStore,
        public_key: &PublicKey,
        operations_to_remove: &[LogIdAndSeqNum],
    ) {
        for (hash, entry) in store.entries.lock().unwrap().iter() {
            if operations_to_remove.contains(&(entry.log_id().as_u64(), entry.seq_num().as_u64()))
                && entry.public_key() == public_key
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
    #[tokio::test]
    async fn errors_when_passed_non_existent_view_id(
        #[from(random_document_view_id)] document_view_id: DocumentViewId,
    ) {
        let store = MemoryStore::default();
        let result = get_checked_document_id_for_view_id(&store, &document_view_id).await;
        assert!(result.is_err());
    }

    #[rstest]
    #[tokio::test]
    async fn gets_document_id_for_view(schema: Schema, operation: Operation) {
        let store = MemoryStore::default();

        // Store one entry and operation in the store.
        let (entry, _) = send_to_store(&store, &operation, &schema, &KeyPair::new())
            .await
            .unwrap();
        let operation_one_id: OperationId = entry.hash().into();

        // Store another entry and operation, from a different public key, which perform an update on
        // the earlier operation.
        let update_operation = OperationBuilder::new(schema.id())
            .action(OperationAction::Update)
            .previous(&operation_one_id.clone().into())
            .fields(&test_fields())
            .build()
            .unwrap();

        let (entry, _) = send_to_store(&store, &update_operation, &schema, &KeyPair::new())
            .await
            .unwrap();
        let operation_two_id: OperationId = entry.hash().into();

        // Get the document id for the passed view id.
        let result = get_checked_document_id_for_view_id(
            &store,
            &DocumentViewId::new(&[operation_one_id.clone(), operation_two_id]),
        )
        .await;

        // Result should be ok.
        assert!(result.is_ok());

        // The returned document id should match the expected one.
        let document_id = result.unwrap();
        assert_eq!(document_id, DocumentId::new(&operation_one_id))
    }

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
    #[case::ok_single_writer(
        &[],
        &[(0, 8)],
        KeyPair::from_private_key_str(PRIVATE_KEY).unwrap()
    )]
    #[case::ok_many_previous(
        &[],
        &[(0, 8), (0, 7), (0, 6)],
        KeyPair::from_private_key_str(PRIVATE_KEY).unwrap()
    )]
    #[case::ok_not_the_most_recent_document_view_id(
        &[],
        &[(0, 1)],
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
    async fn next_args_with_missing_operations(
        #[case] operations_to_remove: &[LogIdAndSeqNum],
        #[case] document_view_id: &[LogIdAndSeqNum],
        #[case] key_pair: KeyPair,
        #[from(populate_store_config)]
        #[with(8, 2, 1)]
        config: PopulateStoreConfig,
    ) {
        let store = MemoryStore::default();
        let (key_pairs, _) = populate_store(&store, &config).await;

        let public_key_with_removed_operations = key_pairs[0].public_key();
        let public_key_making_request = key_pair.public_key();

        // Map the passed &[LogIdAndSeqNum] into a DocumentViewId containing the claimed operations.
        let document_view_id: Vec<OperationId> = document_view_id
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
                            && *entry.public_key() == public_key_with_removed_operations
                    })
                    .map(|entry| entry.hash().into())
            })
            .collect();

        // Construct document view id for previous operations.
        let document_view_id = DocumentViewId::new(&document_view_id);

        // Remove some operations.
        remove_operations(
            &store,
            &public_key_with_removed_operations,
            operations_to_remove,
        );

        // Get the next args.
        let result = next_args(&store, &public_key_making_request, Some(&document_view_id)).await;

        // Unwrap here causing a panic, we check the errors match what we expect.
        result.map_err(|err| err.to_string()).unwrap();
    }

    type SeqNumU64 = u64;
    type LogIdU64 = u64;
    type Backlink = Option<u64>;
    type Skiplink = Option<u64>;

    #[rstest]
    #[case(0, 0, None, (1, 0, None, None))]
    #[case(1, 1, Some((1, 0)), (2, 0, Some(1), None))]
    #[case(2, 1, Some((2, 0)), (3, 0, Some(2), None))]
    #[case(3, 1, Some((3, 0)), (4, 0, Some(3), Some(1)))]
    #[case(4, 1, Some((4, 0)), (5, 0, Some(4), None))]
    #[case(5, 1, Some((5, 0)), (6, 0, Some(5), None))]
    #[case(6, 1, Some((6, 0)), (7, 0, Some(6), None))]
    #[case(7, 1, Some((7, 0)), (8, 0, Some(7), Some(4)))]
    #[case(2, 1, Some((1, 0)), (3, 0, Some(2), None))]
    #[case(3, 1, Some((1, 0)), (4, 0, Some(3), Some(1)))]
    #[case(4, 1, Some((1, 0)), (5, 0, Some(4), None))]
    #[case(5, 1, Some((1, 0)), (6, 0, Some(5), None))]
    #[case(6, 1, Some((1, 0)), (7, 0, Some(6), None))]
    #[case(7, 1, Some((1, 0)), (8, 0, Some(7), Some(4)))]
    #[case(1, 2, None, (1, 2, None, None))]
    #[case(1, 10, None, (1, 10, None, None))]
    #[case(1, 100, None, (1, 100, None, None))]
    #[case(1, 100, Some((1, 9)), (2, 9, Some(1), None))]
    #[case(1, 100, Some((1, 99)), (2, 99, Some(1), None))]
    #[tokio::test]
    async fn next_args_with_expected_results(
        #[case] no_of_entries: usize,
        #[case] no_of_logs: usize,
        #[case] document_view_id: Option<(SeqNumU64, LogIdU64)>,
        #[case] expected_next_args: (SeqNumU64, LogIdU64, Backlink, Skiplink),
    ) {
        let store = MemoryStore::default();
        // Populate the db with the number of entries defined in the test params.
        let config = PopulateStoreConfig {
            no_of_entries,
            no_of_logs,
            no_of_public_keys: 1,
            ..PopulateStoreConfig::default()
        };
        let (key_pairs, _) = populate_store(&store, &config).await;

        // The public key of the author who published the entries.
        let public_key = key_pairs[0].public_key();

        // Construct the passed document view id (specified by sequence number and log id)
        let document_view_id: Option<DocumentViewId> = document_view_id.map(|(seq_num, log_id)| {
            store
                .entries
                .lock()
                .unwrap()
                .values()
                .find(|entry| {
                    entry.seq_num().as_u64() == seq_num && entry.log_id().as_u64() == log_id
                })
                .map(|entry| DocumentViewId::new(&[entry.hash().into()]))
                .unwrap()
        });

        // Construct the expected next args
        let expected_seq_num = SeqNum::new(expected_next_args.0).unwrap();
        let expected_log_id = LogId::new(expected_next_args.1);
        let expected_backlink = match expected_next_args.2 {
            Some(backlink) => store
                .get_entry_at_seq_num(
                    &public_key,
                    &expected_log_id,
                    &SeqNum::new(backlink).unwrap(),
                )
                .await
                .unwrap()
                .map(|entry| entry.hash()),
            None => None,
        };
        let expected_skiplink = match expected_next_args.3 {
            Some(skiplink) => store
                .get_entry_at_seq_num(
                    &public_key,
                    &expected_log_id,
                    &SeqNum::new(skiplink).unwrap(),
                )
                .await
                .unwrap()
                .map(|entry| entry.hash()),
            None => None,
        };

        // Request next args for the public key and document view.
        let (backlink, skiplink, seq_num, log_id) =
            next_args(&store, &public_key, document_view_id.as_ref())
                .await
                .unwrap();

        assert_eq!(backlink, expected_backlink);
        assert_eq!(skiplink, expected_skiplink);
        assert_eq!(seq_num, expected_seq_num);
        assert_eq!(log_id, expected_log_id);
    }

    #[rstest]
    #[tokio::test]
    async fn gets_next_args_other_cases(
        #[from(populate_store_config)]
        #[with(7, 1, 1)]
        config: PopulateStoreConfig,
    ) {
        let store = MemoryStore::default();
        let (key_pairs, documents) = populate_store(&store, &config).await;

        // The public key of the author who published the entries.
        let public_key = key_pairs[0].public_key();

        // Get with no DocumentViewId given.
        let (backlink, skiplink, seq_num, log_id) =
            next_args(&store, &public_key, None).await.unwrap();

        assert_eq!(backlink, None);
        assert_eq!(skiplink, None);
        assert_eq!(seq_num, SeqNum::new(1).unwrap());
        assert_eq!(log_id, LogId::new(1));

        // Get with non-existent DocumentViewId given.
        let result = next_args(&store, &public_key, Some(&random_document_view_id())).await;
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("could not determine document id") // This is a partial string match, preceded by "<Operation xxxxx> not found,"
        );

        // Here we are missing the skiplink.
        remove_entries(&store, &public_key, &[(0, 4)]);
        let document_id = documents.get(0).unwrap();
        let document_view_id = DocumentViewId::new(&[document_id.as_str().parse().unwrap()]);

        let result = next_args(&store, &public_key, Some(&document_view_id)).await;
        assert_eq!(
            result.unwrap_err().to_string(),
            "Expected skiplink entry not found in store: public key 2f8e50c2ede6d936ecc3144187ff1c273808185cfbc5ff3d3748d1ff7353fc96, log id 0, seq num 4"
        );
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
    #[should_panic(expected = "Document is deleted")]
    #[case(KeyPair::from_private_key_str(PRIVATE_KEY).unwrap())]
    #[should_panic(expected = "Document is deleted")]
    #[case(KeyPair::new())]
    #[tokio::test]
    async fn next_args_deleted_documents(
        #[case] key_pair: KeyPair,
        #[from(populate_store_config)]
        #[with(3, 1, 1, true)]
        config: PopulateStoreConfig,
    ) {
        let store = MemoryStore::default();
        let (_, documents) = populate_store(&store, &config).await;

        let document_id = documents.first().unwrap();
        let document_view_id: DocumentViewId = document_id.as_str().parse().unwrap();
        let public_key = key_pair.public_key();

        let result = next_args(&store, &public_key, Some(&document_view_id)).await;

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
    async fn next_args_max_seq_num_reached(
        key_pair: KeyPair,
        #[from(populate_store_config)]
        #[with(2, 1, 1, false)]
        config: PopulateStoreConfig,
    ) {
        let store = MemoryStore::default();
        let _ = populate_store(&store, &config).await;

        let public_key = key_pair.public_key();

        let entry_two = store
            .get_entry_at_seq_num(&public_key, &LogId::default(), &SeqNum::new(2).unwrap())
            .await
            .unwrap()
            .unwrap();

        let encoded_entry = sign_and_encode_entry(
            &LogId::default(),
            &SeqNum::new(u64::MAX).unwrap(),
            Some(&random_hash()),
            Some(&random_hash()),
            entry_two.payload.as_ref().unwrap(),
            &key_pair,
        )
        .unwrap();

        let entry = StorageEntry::new(&encoded_entry, entry_two.payload.as_ref());

        store
            .entries
            .lock()
            .unwrap()
            .insert(entry.hash(), entry.clone());

        let result = next_args(&store, &public_key, Some(&entry_two.hash().into())).await;

        result.map_err(|err| err.to_string()).unwrap();
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
