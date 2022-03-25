// SPDX-License-Identifier: AGPL-3.0-or-later

use thiserror::Error;

use crate::operation::OperationValue;
use crate::schema::SchemaId;

/// Custom error types for system schema views.
#[derive(Error, Debug)]
pub enum SystemSchemaError {
    /// A view can only be created for documents that have not been deleted.
    #[error("unable to create view for deleted document {0}")]
    Deleted(String),

    /// Passed field type does not match the expected type.
    #[error("invalid field \"{0}\" with value {1:#?}")]
    InvalidField(String, OperationValue),

    /// Missing expected field.
    #[error("missing field \"{0}\"")]
    MissingField(String),

    /// Invalid field type found.
    #[error("invalid field type")]
    InvalidFieldType(#[from] crate::schema::FieldTypeError),

    /// A different schema was expected when parsing.
    #[error("expected schema {0:?} got {1:?}")]
    UnexpectedSchema(SchemaId, SchemaId),
}
