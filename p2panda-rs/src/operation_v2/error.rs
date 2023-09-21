// SPDX-License-Identifier: AGPL-3.0-or-later

//! Error types for encoding, decoding and validating operations with schemas and regarding data
//! types like operation fields, relations or plain operations.
use thiserror::Error;

/// Errors from `OperationBuilder` struct.
#[derive(Error, Debug)]
pub enum OperationBuilderError {
    /// Handle errors from `operation_v2::body::validate` module.
    #[error(transparent)]
    ValidateOperationError(#[from] crate::operation_v2::body::error::ValidateOperationError),
}
