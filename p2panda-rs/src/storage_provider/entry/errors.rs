// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::storage_provider::ValidationError;

/// `EntryStore` errors.
#[derive(thiserror::Error, Debug)]
pub enum EntryStorageError {
    /// Catch all error which implementers can use for passing their own errors up the chain.
    #[error("Error occured during `EntryStorage` request in storage provider: {0}")]
    Custom(String),

    /// Error which originates in `determine_skiplink` if the skiplink is missing.
    #[error("Could not find skiplink entry in database")]
    SkiplinkMissing,

    /// Error returned from validating p2panda-rs `EntrySigned` data types.
    #[error(transparent)]
    ValidationError(#[from] ValidationError),
}
