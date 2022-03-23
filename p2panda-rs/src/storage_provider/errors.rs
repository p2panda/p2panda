// SPDX-License-Identifier: AGPL-3.0-or-later

//! Errors for `Storage` provider and associated traits.

use crate::entry::{EntryError, EntrySignedError, LogIdError, SeqNumError};
use crate::hash::HashError;
use crate::identity::AuthorError;
use crate::operation::{OperationEncodedError, OperationError};

/// `StorageProvider` errors which also handle errors originating in LogStorage and EntryStorage.
#[derive(thiserror::Error, Debug)]
pub enum StorageProviderError {
    /// Catch all error which implementers can use for passing their own errors
    /// up the chain.
    #[error("Error occured in `StorageProvider`: {0}")]
    Custom(String),

    /// Error returned from validating p2panda-rs `Author` data types.
    #[error(transparent)]
    AuthorValidation(#[from] AuthorError),

    /// Error returned from validating p2panda-rs `Hash` data types.
    #[error(transparent)]
    HashValidation(#[from] HashError),

    /// Error returned from validating p2panda-rs `Entry` data types.
    #[error(transparent)]
    EntryValidation(#[from] EntryError),

    /// Error returned from validating p2panda-rs `EntrySigned` data types.
    #[error(transparent)]
    EntrySignedValidation(#[from] EntrySignedError),

    /// Error returned from validating p2panda-rs `Operation` data types.
    #[error(transparent)]
    OperationValidation(#[from] OperationError),

    /// Error returned from validating p2panda-rs `OperationEncoded` data types.
    #[error(transparent)]
    OperationEncodedValidation(#[from] OperationEncodedError),

    /// Error returned from validating p2panda-rs `LogId` data types.
    #[error(transparent)]
    LogIdValidation(#[from] LogIdError),

    /// Error returned from validating p2panda-rs `SeqNum` data types.
    #[error(transparent)]
    SeqNumValidation(#[from] SeqNumError),

    /// Error returned from validating Bamboo entries.
    #[error(transparent)]
    BambooValidation(#[from] bamboo_rs_core_ed25519_yasmf::verify::Error),

    /// Error returned from `panda_publishEntry` RPC method.
    #[error(transparent)]
    PublishEntryError(#[from] PublishEntryError),

    /// Error returned from `LogStorage` methods.
    #[error(transparent)]
    LogStorageError(#[from] LogStorageError),

    /// Error returned from `EntryStorage` methods.
    #[error(transparent)]
    EntryStorageError(#[from] EntryStorageError),
}

/// `LogStorage` errors.
#[derive(thiserror::Error, Debug)]
pub enum LogStorageError {
    /// Catch all error which implementers can use for passing their own errors
    /// up the chain.
    #[error("Error occured during `LogStorage` request in storage provider: {0}")]
    Custom(String),
}

/// `EntryStorage` errors.
#[derive(thiserror::Error, Debug)]
pub enum EntryStorageError {
    /// Catch all error which implementers can use for passing their own errors
    /// up the chain.
    #[error("Error occured during `EntryStorage` request in storage provider: {0}")]
    Custom(String),

    /// Error which originates in `determine_skiplink` if the skiplink is missing.
    #[error("Could not find skiplink entry in database")]
    SkiplinkMissing,
}

/// Errors which can occur in a call to `publish_entry()`..
#[derive(thiserror::Error, Debug)]
#[allow(missing_copy_implementations, missing_docs)]
pub enum PublishEntryError {
    #[error("Could not find backlink entry in database")]
    BacklinkMissing,

    #[error("Could not find skiplink entry in database")]
    SkiplinkMissing,

    #[error("Could not find document hash for entry in database")]
    DocumentMissing,

    #[error("UPDATE or DELETE operation came with an entry without backlink")]
    OperationWithoutBacklink,

    #[error("Requested log id {0} does not match expected log id {1}")]
    InvalidLogId(u64, u64),

    #[error("Invalid Entry and Operation pair passed to `publish_entry()`")]
    InvalidEntryWithOperation,
}
