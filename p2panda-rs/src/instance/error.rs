// SPDX-License-Identifier: AGPL-3.0-or-later

use thiserror::Error;

/// Custom error types for `Instance`.
#[derive(Error, Debug)]
pub enum InstanceError {
    /// TryFrom operation must be CREATE.
    #[error("operation must be CREATE")]
    NotCreateOperation,

    /// Operation passed to `update()` must be UPDATE.
    #[error("operation must be UPDATE")]
    NotUpdateOperation,

    /// Validation error.
    #[error("error while creating instance")]
    ValidationError(#[from] crate::schema::SchemaError),
}
