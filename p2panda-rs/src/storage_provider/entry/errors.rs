// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::hash::Hash;
use crate::storage_provider::ValidationError;

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
