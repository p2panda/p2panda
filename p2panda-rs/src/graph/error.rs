// SPDX-License-Identifier: AGPL-3.0-or-later

use thiserror::Error;

/// Error types for methods of `materialiser` module.
#[derive(Error, Debug)]
#[allow(missing_copy_implementations)]
pub enum GraphError {
    /// Cycle detected in graph.
    #[error("Cycle detected")]
    CycleDetected,

    /// Cycle detected or graph missing dependencies.
    #[error("Badly formed graph")]
    BadlyFormedGraph,

    /// No root node found in graph.
    #[error("No root node found")]
    NoRootNode,

    /// There can't be more than one root node in a graph.
    #[error("Multiple root nodes found")]
    MultipleRootNodes,

    /// Requested node not found in graph.
    #[error("Node not found in graph")]
    NodeNotFound,

    /// Passed to nodes array is invalid.
    #[error("Invalid to nodes array passed")]
    InvalidToNodesPassed,
}
