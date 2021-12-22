// SPDX-License-Identifier: AGPL-3.0-or-later

use thiserror::Error;

/// Error types for methods of `DocumentBuilder` struct.
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

    /// Handle errors from validating CBOR schemas.
    #[error(transparent)]
    SchemaError(#[from] crate::schema::SchemaError),
}

/// Error types for methods of `Document` struct.
#[allow(missing_copy_implementations)]
#[derive(Error, Debug)]
pub enum DocumentError {
    /// No create operation found.
    #[error("Every document must contain one create operation")]
    NoCreateOperation,

    /// No operation found.
    #[error("No operation found with that id")]
    OperationNotFound,

    /// Internal IncrementalTopo error.
    #[error("Error sorting graph")]
    IncrementalTopoError,

    /// Get operation error.
    #[error("Operation with that id does not exist")]
    OperationDoesNotExist,

    /// Handle errors from validating CBOR schemas.
    #[error(transparent)]
    InstanceError(#[from] crate::instance::InstanceError),
}
