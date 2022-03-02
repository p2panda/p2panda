// SPDX-License-Identifier: AGPL-3.0-or-later

use thiserror::Error;

/// Custom errors related to `SchemaId`.
#[derive(Error, Debug)]
pub enum SchemaIdError {
    /// Invalid hash in schema id.
    #[error("invalid hash string")]
    HashError(#[from] crate::hash::HashError),
}
