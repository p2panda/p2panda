
// SPDX-License-Identifier: AGPL-3.0-or-later

use thiserror::Error;

/// Error types for `Hash`.
#[derive(Error, Debug)]
#[allow(missing_copy_implementations)]
pub enum KeyGroupError {
    #[error("invalid membership: {0}")]
    InvalidMembership(String),

    #[error("duplicate member: {0}")]
    DuplicateMembership(String),

    #[error("key group must have at least one member")]
    NoMemberships,

    #[error("unexpected multiple owner fields in document {0}")]
    MultipleOwners(String),

    #[error(transparent)]
    ParsingError(#[from] crate::schema::system::SystemSchemaError),
}
