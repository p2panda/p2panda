// SPDX-License-Identifier: AGPL-3.0-or-later

use thiserror::Error;

/// Custom error types for schema validation.
#[derive(Error, Debug)]
pub enum SchemaError {
    /// Message contains invalid fields.
    #[error("invalid message schema: {0}")]
    InvalidSchema(String),

    /// Message can't be deserialized from invalid CBOR encoding.
    #[error("invalid CBOR format")]
    InvalidCBOR,

    /// There is no schema set
    #[error("no CDDL schema present")]
    NoSchema,

    /// Error while parsing CDDL
    #[error("error while parsing CDDL: {0}")]
    ParsingError(String),

    /// Message validation error
    #[error("invalid message values")]
    ValidationError(String),

    /// Message fields error
    #[error("error while adding message fields")]
    MessageFieldsError(#[from] crate::message::MessageFieldsError),

    /// Message error
    #[error("error while creating message")]
    MessageError(#[from] crate::message::MessageError),
}
