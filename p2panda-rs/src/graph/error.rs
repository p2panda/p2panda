// SPDX-License-Identifier: AGPL-3.0-or-later

use thiserror::Error;

/// Error types for methods of `GraphNode` struct.
#[allow(missing_copy_implementations)]
#[derive(Error, Debug)]
pub enum GraphNodeError {
    /// Invalid attempt to create a graph node with invalid operation with meta.
    #[error(transparent)]
    OperationWithMetaError(#[from] crate::operation::OperationWithMetaError),
}
