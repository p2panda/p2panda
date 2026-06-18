// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_store::SqliteError;
use thiserror::Error;

/// Errors which can be returned from stores.
#[derive(Debug, Error)]
#[error("{0}")]
pub struct StoreError(String);

impl From<String> for StoreError {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<SqliteError> for StoreError {
    fn from(err: SqliteError) -> Self {
        Self(err.to_string())
    }
}
