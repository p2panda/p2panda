// SPDX-License-Identifier: AGPL-3.0-or-later

use thiserror::Error;

/// Error types for methods of `GraphNode` struct.
#[allow(missing_copy_implementations)]
#[derive(Error, Debug)]
pub enum DocumentBuilderError {
    /// No create operation found.
    #[error("Every document must contain one create operation")]
    NoCreateOperation,

    /// A document can only have one create operation.
    #[error("Multiple create operations found")]
    MoreThanOneCreateOperation,

    /// Internal IncrementalTopo error.
    #[error("Error adding dependency to graph")]
    IncrementalTopoDepenedencyError,
}
