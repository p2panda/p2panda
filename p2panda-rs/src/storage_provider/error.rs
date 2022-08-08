// SPDX-License-Identifier: AGPL-3.0-or-later

//! Errors from storage provider and associated traits.
use crate::document::{DocumentId, DocumentViewId};
use crate::entry::error::{LogIdError, SeqNumError, ValidateEntryError};
use crate::hash::error::HashError;
use crate::hash::Hash;
use crate::identity::error::AuthorError;
use crate::operation::error::ValidateOperationError;
use crate::operation::OperationId;

/// Data validation errors which can occur in the storage traits.
#[derive(thiserror::Error, Debug)]
pub enum ValidationError {
    /// Error returned from validating p2panda-rs `Author` data types.
    #[error(transparent)]
    AuthorValidation(#[from] AuthorError),

    /// Error returned from validating p2panda-rs `Hash` data types.
    #[error(transparent)]
    HashValidation(#[from] HashError),

    /// Error returned from validating p2panda-rs `Entry` data types.
    #[error(transparent)]
    EntryValidation(#[from] ValidateEntryError),

    /// Error returned from validating p2panda-rs `Operation` data types.
    #[error(transparent)]
    OperationValidation(#[from] ValidateOperationError),

    /// Error returned from validating p2panda-rs `LogId` data types.
    #[error(transparent)]
    LogIdValidation(#[from] LogIdError),

    /// Error returned from validating p2panda-rs `SeqNum` data types.
    #[error(transparent)]
    SeqNumValidation(#[from] SeqNumError),

    /// Error returned from validating Bamboo entries.
    #[error(transparent)]
    BambooValidation(#[from] bamboo_rs_core_ed25519_yasmf::verify::Error),
}

/// `LogStorage` errors.
#[derive(thiserror::Error, Debug)]
pub enum LogStorageError {
    /// Catch all error which implementers can use for passing their own errors up the chain.
    #[error("Error occured during `LogStorage` request in storage provider: {0}")]
    Custom(String),
}

/// `EntryStorage` errors.
#[derive(thiserror::Error, Debug)]
pub enum EntryStorageError {
    /// Catch all error which implementers can use for passing their own errors up the chain.
    #[error("Error occured during `EntryStorage` request in storage provider: {0}")]
    Custom(String),

    /// Error which occurs if entries' expected backlink is missing from the database.
    #[error("Could not find expected backlink in database for entry with id: {0}")]
    ExpectedBacklinkMissing(Hash),

    /// Error which occurs if entries' encoded backlink hash does not match the expected one
    /// present in the database.
    #[error(
        "The backlink hash encoded in the entry: {0} did not match the expected backlink hash"
    )]
    InvalidBacklinkPassed(Hash),

    /// Error which occurs if entries' expected skiplink is missing from the database.
    #[error("Could not find expected skiplink in database for entry with id: {0}")]
    ExpectedSkiplinkMissing(Hash),

    /// Error which occurs if entries' encoded skiplink hash does not match the expected one
    /// present in the database.
    #[error("The skiplink hash encoded in the entry: {0} did not match the known hash of the skiplink target")]
    InvalidSkiplinkPassed(Hash),

    /// Error which originates in `determine_skiplink` if the expected skiplink is missing.
    #[error("Could not find expected skiplink entry in database")]
    ExpectedNextSkiplinkMissing,

    /// Error which originates in `get_all_skiplink_entries_for_entry` if an entry in
    /// the requested cert pool is missing.
    #[error("Entry required for requested certificate pool missing at seq num: {0}")]
    CertPoolEntryMissing(u64),

    /// Error returned from validating p2panda-rs `EntrySigned` data types.
    #[error(transparent)]
    ValidationError(#[from] ValidationError),
}

/// `OperationStore` errors.
#[derive(thiserror::Error, Debug)]
pub enum OperationStorageError {
    /// Catch all error which implementers can use for passing their own errors up the chain.
    #[error("Error occured in OperationStore: {0}")]
    Custom(String),

    /// A fatal error occured when performing a storage query.
    #[error("A fatal error occured in OperationStore: {0}")]
    FatalStorageError(String),

    /// Error returned when insertion of an operation is not possible due to database constraints.
    #[error("Error occured when inserting an operation with id {0:?} into storage")]
    InsertionError(OperationId),
}

/// `DocumentStore` errors.
#[derive(thiserror::Error, Debug)]
pub enum DocumentStorageError {
    /// Catch all error which implementers can use for passing their own errors up the chain.
    #[error("Error occured in DocumentStore: {0}")]
    Custom(String),

    /// A fatal error occured when performing a storage query.
    #[error("A fatal error occured in DocumentStore: {0}")]
    FatalStorageError(String),

    /// Error which originates in `insert_document_view()` when the insertion fails.
    #[error("Error occured when inserting a document view with id {0:?} into storage")]
    DocumentViewInsertionError(DocumentViewId),

    /// Error which originates in `insert_document()` when the insertion fails.
    #[error("Error occured when inserting a document with id {0:?} into storage")]
    DocumentInsertionError(DocumentId),
}
