// SPDX-License-Identifier: AGPL-3.0-or-later

use thiserror::Error;

/// Custom error types for instance.
#[derive(Error, Debug)]
pub enum InstanceError {
    /// TryFrom operation must be CREATE.
    #[error("operation must be CREATE")]
    NotCreateOperation,

    /// Validation error
    #[error("error while creating instance")]
    ValidationError(#[from] crate::schema::SchemaError),
}
