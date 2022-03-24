// SPDX-License-Identifier: AGPL-3.0-or-later

use thiserror::Error;

/// Error types for `Hash`.
#[derive(Error, Debug)]
#[allow(missing_copy_implementations)]
pub enum KeyGroupError {
    /// A public key can only have one membership in a key group.
    #[error("duplicate member: {0}")]
    DuplicateMembership(String),

    /// Memberships must have matching and valid request and response.
    #[error("invalid membership: {0}")]
    InvalidMembership(String),

    /// Key groups must have a name.
    #[error("invalid key group name '{0}'")]
    InvalidName(String),

    /// Authorised documents must not have more than one owner.
    #[error("unexpected multiple owner fields in document {0}")]
    MultipleOwners(String),

    /// The [`KeyGroupView`] must be supplied when creating key groups from document collections.
    #[error("key group to be created not found among supplied documents")]
    MissingKeyGroupView,

    /// Key group instances must have members.
    #[error("key group must have at least one member")]
    NoMemberships,

    /// All key groups that are members must be passed to the [`KeyGroup`] constructor.
    #[error("key group is a member but was not included in parameters: {0}")]
    MissingMemberKeyGroup(String),

    /// Error from parsing system schema.
    #[error(transparent)]
    ParsingError(#[from] crate::schema::system::SystemSchemaError),
}
