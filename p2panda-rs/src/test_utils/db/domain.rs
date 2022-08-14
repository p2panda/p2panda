// SPDX-License-Identifier: AGPL-3.0-or-later

use bamboo_rs_core_ed25519_yasmf::entry::is_lipmaa_required;

use crate::document::DocumentViewId;
use crate::entry::decode::decode_entry;
use crate::entry::traits::{AsEncodedEntry, AsEntry};
use crate::entry::{EncodedEntry, Entry, LogId, SeqNum};
use crate::hash::Hash;
use crate::identity::Author;
use crate::operation::plain::PlainOperation;
use crate::operation::traits::AsOperation;
use crate::operation::validate::validate_operation_with_entry;
use crate::operation::{EncodedOperation, OperationAction};
use crate::schema::Schema;
use crate::storage_provider::traits::{AsStorageLog, StorageProvider};
use crate::storage_provider::utils::Result;
use crate::test_utils::db::validation::{
    ensure_document_not_deleted, get_expected_skiplink, increment_seq_num, is_next_seq_num,
    next_log_id, verify_log_id,
};
use crate::test_utils::db::EntryArgsResponse;
use crate::Human;

use super::validation::get_checked_document_id_for_view_id;

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
) -> Result<EntryArgsResponse> {
    // Init the next args with base default values.
    let mut next_args = EntryArgsResponse {
        backlink: None,
        skiplink: None,
        seq_num: SeqNum::default(),
        log_id: LogId::default(),
    };

    ////////////////////////
    // HANDLE CREATE CASE //
    ////////////////////////

    // If no document_view_id is passed then this is a request for publishing a CREATE operation
    // and we return the args for the next free log by this author.
    if document_view_id.is_none() {
        let log_id = next_log_id(store, public_key).await?;
        next_args.log_id = log_id;
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
    let log_id = store.get(public_key, &document_id).await?;

    // Check if an existing log id was found for this author and document.
    match log_id {
        // If it wasn't found, we just calculate the next log id safely and return the next args.
        None => {
            let next_log_id = next_log_id(store, public_key).await?;
            next_args.log_id = next_log_id
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
                    let mut latest_seq_num = latest_entry.seq_num().to_owned();
                    increment_seq_num(&mut latest_seq_num)
                }
                None => Ok(SeqNum::default()),
            }
            .unwrap_or_else(|_| {
                panic!(
                    "Max sequence number reached for {} log {}",
                    public_key.display(),
                    log_id.as_u64()
                )
            });

            // Check if skiplink is required and if it is get the entry and return its hash.
            let skiplink = if is_lipmaa_required(seq_num.as_u64()) {
                // Determine skiplink ("lipmaa"-link) entry in this log.
                Some(get_expected_skiplink(store, public_key, &log_id, &seq_num).await?)
            } else {
                None
            }
            .map(|entry| entry.hash());

            next_args.backlink = latest_entry.map(|entry| entry.hash());
            next_args.skiplink = skiplink;
            next_args.seq_num = seq_num;
            next_args.log_id = log_id;
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
    schema: &Schema,
    encoded_entry: &EncodedEntry,
    plain_operation: &PlainOperation,
    encoded_operation: &EncodedOperation,
) -> Result<EntryArgsResponse> {
    //////////////////
    // DECODE ENTRY //
    //////////////////

    let entry = decode_entry(encoded_entry)?;
    let author = entry.public_key();
    let log_id = entry.log_id();
    let seq_num = entry.seq_num();

    //////////////////////////////////
    // VALIDATE ENTRY AND OPERATION //
    //////////////////////////////////

    // TODO: Check this validation flow is still correct.

    // Verify that the claimed seq num matches the expected seq num for this author and log.
    let latest_entry = store.get_latest_entry(author, log_id).await?;
    let latest_seq_num = latest_entry.as_ref().map(|entry| entry.seq_num());
    is_next_seq_num(latest_seq_num, seq_num)?;

    // The backlink for this entry is the latest entry from this public key's log.
    let backlink = latest_entry;

    // If a skiplink is claimed, get the expected skiplink from the database, errors
    // if it can't be found.
    let skiplink = match entry.skiplink() {
        Some(_) => Some(get_expected_skiplink(store, author, log_id, seq_num).await?),
        None => None,
    };

    let skiplink_params: Option<(Entry, Hash)> = skiplink.map(|entry| {
        let hash = entry.hash();
        (entry.into(), hash)
    });

    let backlink_params: Option<(Entry, Hash)> = backlink.map(|entry| {
        let hash = entry.hash();
        (entry.into(), hash)
    });

    // Perform validation of the entry and it's operation.
    let operation = validate_operation_with_entry(
        &entry,
        encoded_entry,
        skiplink_params.as_ref().map(|(entry, hash)| (entry, hash)),
        backlink_params.as_ref().map(|(entry, hash)| (entry, hash)),
        plain_operation,
        encoded_operation,
        schema,
    )?;

    ///////////////////////////////////
    // ENSURE SINGLE NODE PER AUTHOR //
    ///////////////////////////////////

    // @TODO: Missing a step here where we check if the author has published to this node before, and also
    // if we know of any other nodes they have published to. Not sure how to do this yet.

    //////////////////////////
    // DETERMINE DOCUMENT ID //
    //////////////////////////

    let document_id = match operation.action() {
        OperationAction::Create => {
            // Derive the document id for this new document.
            encoded_entry.hash().into()
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
    verify_log_id(store, author, log_id, &document_id).await?;

    ///////////////
    // STORE LOG //
    ///////////////

    // If this is the first entry in a new log we insert it here.
    if entry.seq_num().is_first() {
        let log = S::StorageLog::new(author, &operation.schema_id(), &document_id, log_id);

        store.insert_log(log).await?;
    }

    /////////////////////////////////////
    // DETERMINE NEXT ENTRY ARG VALUES //
    /////////////////////////////////////

    // If we have reached MAX_SEQ_NUM here for the next args then we will error and _not_ store the
    // entry which is being processed in this request.
    let next_seq_num = increment_seq_num(&mut seq_num.clone()).map_err(|_| {
        format!(
            "Max sequence number reached for {} log {}",
            author.display(),
            log_id.as_u64()
        )
    })?;
    let backlink = Some(encoded_entry.hash());

    // Check if skiplink is required and return hash if so
    let skiplink = if is_lipmaa_required(next_seq_num.as_u64()) {
        Some(get_expected_skiplink(store, author, log_id, &next_seq_num).await?)
    } else {
        None
    }
    .map(|entry| entry.hash());

    let next_args = EntryArgsResponse {
        log_id: log_id.to_owned(),
        seq_num: next_seq_num,
        backlink,
        skiplink,
    };

    ///////////////////////////////
    // STORE ENTRY AND OPERATION //
    ///////////////////////////////

    // Insert the entry into the store.
    store
        .insert_entry(&entry, encoded_entry, Some(encoded_operation))
        .await?;
    // Insert the operation into the store.
    store.insert_operation(&operation, &document_id).await?;

    Ok(next_args)
}

// @TODO: Re-instate the tests from `domain` module in `aquadoggo`, they are compatible.
//
// They are in commit 9a4f4a35ab60b79b814e7f771cdc7a4bac6281f5 if you wanna see.
