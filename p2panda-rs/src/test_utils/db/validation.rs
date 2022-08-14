// SPDX-License-Identifier: AGPL-3.0-or-later

//! Methods for validating entries and operations against expected and stored values.
use std::collections::HashSet;

use crate::document::{DocumentId, DocumentViewId};
use crate::entry::{LogId, SeqNum};
use crate::identity::Author;
use crate::operation::traits::AsOperation;
use crate::storage_provider::error::{EntryStorageError, LogStorageError, OperationStorageError};
use crate::storage_provider::traits::StorageProvider;
use crate::Human;

/// Error type used in the validation module.
#[derive(thiserror::Error, Debug)]
pub enum ValidationError {
    /// Helper error type used in validation module.
    #[error("{0}")]
    Custom(String),

    /// Error coming from the log store.
    #[error(transparent)]
    LogStoreError(#[from] LogStorageError),

    /// Error coming from the entry store.
    #[error(transparent)]
    EntryStoreError(#[from] EntryStorageError),

    /// Error coming from the operation store.
    #[error(transparent)]
    OperationStoreError(#[from] OperationStorageError),
}

/// Verify that a claimed seq num is the next sequence number following the latest.
///
/// Performs two steps:
/// - determines the expected sequence number
///     - if `latest_seq_num` is `Some` by incrementing that
///     - if `latest_seq_num` is `None` by setting it to 1
/// - ensures the claimed sequence number is equal to the expected one.
pub fn is_next_seq_num(
    latest_seq_num: Option<&SeqNum>,
    claimed_seq_num: &SeqNum,
) -> Result<(), ValidationError> {
    let expected_seq_num = match latest_seq_num {
        Some(seq_num) => {
            let mut seq_num = seq_num.to_owned();
            increment_seq_num(&mut seq_num)
        }
        None => Ok(SeqNum::default()),
    }?;

    if expected_seq_num != *claimed_seq_num {
        return Err(ValidationError::Custom(format!("Entry's claimed seq num of {} does not match expected seq num of {} for given author and log",
        claimed_seq_num.as_u64(),
        expected_seq_num.as_u64())));
    };
    Ok(())
}

/// Verify that a log id is correctly chosen for a pair of author and document id.
///
/// This method handles both the case where the claimed log id already exists for this author
/// and where it is a new log.
///
/// The following steps are taken:
/// - Retrieve the stored log id for the document id
///   - If found, ensure it matches the claimed log id
///   - If not found retrieve the next available log id for this author and ensure that matches
pub async fn verify_log_id<S: StorageProvider>(
    store: &S,
    author: &Author,
    claimed_log_id: &LogId,
    document_id: &DocumentId,
) -> Result<(), ValidationError> {
    // Check if there is a log id registered for this document and public key already in the store.
    match store.get(author, document_id).await? {
        Some(expected_log_id) => {
            // If there is, check it matches the log id encoded in the entry
            if *claimed_log_id != expected_log_id {
                return Err(ValidationError::Custom(format!(
                    "Entry's claimed log id of {} does not match existing log id of {} for given author and document",
                    claimed_log_id.as_u64(),
                    expected_log_id.as_u64()
                )));
            }
        }
        None => {
            // If there isn't, check that the next log id for this author matches the one encoded in
            // the entry.
            let expected_log_id = next_log_id(store, author).await?;

            if *claimed_log_id != expected_log_id {
                return Err(ValidationError::Custom(format!(
                    "Entry's claimed log id of {} does not match expected next log id of {} for given author",
                    claimed_log_id.as_u64(),
                    expected_log_id.as_u64()
                )));
            }
        }
    };
    Ok(())
}

/// Get the entry that _should_ be the skiplink target for the given author, log id and seq num.
///
/// This method determines the expected skiplink given an author, log id and sequence number. It
/// _does not_ verify that this matches the skiplink encoded on any entry.
///
/// An error is returned if:
/// - seq num 1 was passed in, which can not have a skiplink
/// - the expected skiplink target could not be found in the database.
pub async fn get_expected_skiplink<S: StorageProvider>(
    store: &S,
    author: &Author,
    log_id: &LogId,
    seq_num: &SeqNum,
) -> Result<S::Entry, ValidationError> {
    if seq_num.is_first() {
        return Err(ValidationError::Custom(
            "Entry with seq num 1 can not have skiplink".to_string(),
        ));
    };

    // Unwrap because method always returns `Some` for seq num > 1
    let skiplink_seq_num = seq_num.skiplink_seq_num().unwrap();

    let skiplink_entry = store
        .get_entry_at_seq_num(author, log_id, &skiplink_seq_num)
        .await?;

    match skiplink_entry {
        Some(entry) => Ok(entry),
        None => Err(ValidationError::Custom(format!(
            "Expected skiplink target not found in store: {}, log id {}, seq num {}",
            author.display(),
            log_id.as_u64(),
            skiplink_seq_num.as_u64(),
        ))),
    }
}

/// Ensure that a document is not deleted.
///
/// Takes the following steps:
/// - retrieve all operations for the given document id
/// - ensure none of them contain a DELETE action
pub async fn ensure_document_not_deleted<S: StorageProvider>(
    store: &S,
    document_id: &DocumentId,
) -> Result<(), ValidationError> {
    // Retrieve the document view for this document, if none is found, then it is deleted.
    let operations = store.get_operations_by_document_id(document_id).await?;
    if operations.iter().any(|operation| operation.is_delete()) {
        return Err(ValidationError::Custom("Document is deleted".to_string()));
    };
    Ok(())
}

/// Retrieve the next log id for a given author.
///
/// Takes the following steps:
/// - retrieve the latest log id for the given author
/// - safely increment it by 1
pub async fn next_log_id<S: StorageProvider>(
    store: &S,
    author: &Author,
) -> Result<LogId, ValidationError> {
    let latest_log_id = store.latest_log_id(author).await?;

    match latest_log_id {
        Some(mut log_id) => increment_log_id(&mut log_id),
        None => Ok(LogId::default()),
    }
}

/// Safely increment a sequence number by one.
pub fn increment_seq_num(seq_num: &mut SeqNum) -> Result<SeqNum, ValidationError> {
    match seq_num.next() {
        Some(next_seq_num) => Ok(next_seq_num),
        None => Err(ValidationError::Custom(
            "Max sequence number reached".to_string(),
        )),
    }
}

/// Safely increment a log id by one.
pub fn increment_log_id(log_id: &mut LogId) -> Result<LogId, ValidationError> {
    match log_id.next() {
        Some(next_log_id) => Ok(next_log_id),
        None => Err(ValidationError::Custom("Max log id reached".to_string())),
    }
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
) -> Result<DocumentId, ValidationError> {
    let mut found_document_ids: HashSet<DocumentId> = HashSet::new();
    for operation in view_id.iter() {
        // If any operation can't be found return an error at this point already.
        let document_id = store.get_document_by_operation_id(operation).await?;

        if document_id.is_none() {
            return Err(ValidationError::Custom(format!(
                "{} not found, could not determine document id",
                operation.display()
            )));
        }

        found_document_ids.insert(document_id.unwrap());
    }

    // We can unwrap here as there must be at least one document view else the error above would
    // have been triggered.
    let mut found_document_ids_iter = found_document_ids.iter();
    let document_id = found_document_ids_iter.next().unwrap();

    if found_document_ids_iter.next().is_some() {
        return Err(ValidationError::Custom("Invalid document view id: operations in passed document view id originate from different documents".to_string()));
    };

    Ok(document_id.to_owned())
}

// @TODO: Re-instate the tests from `validation` module in `aquadoggo`, they are compatible.
