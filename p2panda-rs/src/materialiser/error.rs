// SPDX-License-Identifier: AGPL-3.0-or-later

use thiserror::Error;

/// Error types for methods of `materialiser` module.
#[derive(Error, Debug)]
#[allow(missing_copy_implementations)]
pub enum MaterialisationError {
    /// Handle errors from `EntrySignedError` struct.
    #[error(transparent)]
    EntrySignedError(#[from] crate::entry::EntrySignedError),

    /// Handle errors from `OperationSignedError` struct.
    #[error(transparent)]
    OperationSignedError(#[from] crate::operation::OperationSignedError),
}
