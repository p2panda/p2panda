// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::document::DocumentId;
use crate::entry::error::DecodeEntryError;
use crate::identity::PublicKey;
use crate::operation::error::ValidateOperationError;
use crate::operation::OperationId;
use crate::schema::SchemaId;
use crate::storage_provider::error::{EntryStorageError, LogStorageError, OperationStorageError};

/// Error type used in the validation module.
#[derive(thiserror::Error, Debug)]
pub enum ValidationError {
    /// The claimed sequence number didn't match the expected.
    #[error("Entry's claimed seq num of {0} does not match expected seq num of {1} for given public key and log")]
    SeqNumDoesNotMatch(u64, u64),

    /// The expected skiplink entry wasn't found in the store.
    #[error("Expected skiplink entry not found in store: public key {0}, log id {1}, seq num {2}")]
    ExpectedSkiplinkNotFound(String, u64, u64),

    /// The claimed log id didn't match the expected for a given public key and document id.
    #[error(
        "Entry's claimed log id of {0} does not match existing log id of {1} for given public key and document id"
    )]
    LogIdDoesNotMatchExisting(u64, u64),

    /// The expected log for given public key and document id not found.
    #[error("Expected log not found in store for: public key {0}, document id {1}")]
    ExpectedDocumentLogNotFound(PublicKey, DocumentId),

    /// Claimed log id is already in use.
    #[error(
        "Entry's claimed log id of {0} is already in use for given public key"
    )]
    LogIdDuplicate(u64),

    /// Entry with seq num 1 contained a skiplink.
    #[error("Entry with seq num 1 can not have skiplink")]
    FirstEntryWithSkiplink,

    /// Claimed schema does not match the documents expected schema.
    #[error("Operation {0} claims incorrect schema {1} for document with schema {2}")]
    InvalidClaimedSchema(OperationId, SchemaId, SchemaId),

    /// This document is deleted.
    #[error("Document is deleted")]
    DocumentDeleted,

    /// Max u64 sequence number reached.
    #[error("Max sequence number reached")]
    MaxSeqNum,

    /// Max u64 log id reached.
    #[error("Max log id reached")]
    MaxLogId,

    /// An operation in the `previous` field was not found in the store.
    #[error("Previous operation {0} not found in store")]
    PreviousOperationNotFound(OperationId),

    /// A document view id was provided which contained operations from different documents.
    #[error("Operations in passed document view id originate from different documents")]
    InvalidDocumentViewId,

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

/// Error type used in the domain module.
#[derive(thiserror::Error, Debug)]
pub enum DomainError {
    /// The maximum u64 sequence number has been reached for the public key and log id combination.
    #[error("Max sequence number reached for public key {0} log {1}")]
    MaxSeqNumReached(String, u64),

    /// Tried to update or delete a deleted document.
    #[error("You are trying to update or delete a document which has been deleted")]
    DeletedDocument,

    /// Expected log id not found when calculating next args.
    #[error("Expected log id {0} not found when calculating next args")]
    ExpectedLogIdNotFound(u64),

    /// Validation errors.
    #[error(transparent)]
    ValidationError(#[from] ValidationError),

    /// Error coming from the log store.
    #[error(transparent)]
    LogStoreError(#[from] LogStorageError),

    /// Error coming from the entry store.
    #[error(transparent)]
    EntryStoreError(#[from] EntryStorageError),

    /// Error coming from the operation store.
    #[error(transparent)]
    OperationStoreError(#[from] OperationStorageError),

    /// Error occurring when decoding entries.
    #[error(transparent)]
    DecodeEntryError(#[from] DecodeEntryError),

    /// Error occurring when validating operations.
    #[error(transparent)]
    ValidateOperationError(#[from] ValidateOperationError),
}
