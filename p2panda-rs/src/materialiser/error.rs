// SPDX-License-Identifier: AGPL-3.0-or-later

use thiserror::Error;

/// Error types for methods of `materialiser` module.
#[derive(Error, Debug)]
#[allow(missing_copy_implementations)]
pub enum MaterialisationError {
    /// Handle errors from `EntrySignedError` struct.
    #[error(transparent)]
    EntrySignedError(#[from] crate::entry::EntrySignedError),

    /// Handle errors from `OperationWithMetaError` struct.
    #[error(transparent)]
    OperationWithMetaError(#[from] crate::operation::OperationWithMetaError),
}

/// Error types for methods of `materialiser` module.
#[derive(Error, Debug)]
#[allow(missing_copy_implementations)]
pub enum GraphError {
    /// Cycle detected in graph.
    #[error("Cycle detected")]
    CycleDetected,

    /// Cycle detected in graph.
    #[error("Badly formed graph")]
    BadlyFormedGraph,
}
