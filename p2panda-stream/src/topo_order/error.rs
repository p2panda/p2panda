// SPDX-License-Identifier: AGPL-3.0-or-later

//! Error types for creating and traversing an operation graph.
use thiserror::Error;

/// Error types for methods of `graph` module.
#[derive(Error, Debug, Clone)]
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

    /// Requested trim nodes not found in graph.
    #[error("Requested trim nodes not found in graph")]
    InvalidTrimNodes,

    /// Requested trim nodes not found in graph.
    #[error(transparent)]
    ReducerError(#[from] ReducerError),
}

/// Error types for `Reducer` trait.
#[derive(Error, Debug, Clone)]
#[allow(missing_copy_implementations)]
pub enum ReducerError {
    /// Error occurred when performing reducer function.
    #[error("Could not perform reducer function: {0}")]
    Custom(String),
}
