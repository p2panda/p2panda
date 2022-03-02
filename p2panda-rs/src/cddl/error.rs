// SPDX-License-Identifier: AGPL-3.0-or-later

use thiserror::Error;

/// Error types for schema validation.
#[derive(Error)]
pub enum CDDLValidationError {
    /// Operation contains invalid fields.
    // Note: We pretty-print the vector of error strings to get line breaks
    #[error("invalid operation schema: {0:#?}")]
    InvalidSchema(Vec<String>),

    /// Operation can't be deserialised from invalid CBOR encoding.
    #[error("invalid CBOR format")]
    InvalidCBOR,

    /// Attempted to validate an operation using an invalid CDDL definition
    #[error("invalid CDDL definition: {0}")]
    InvalidCDDL(String),

    /// There is no schema set.
    #[error("no CDDL schema present")]
    NoSchema,

    /// Error while parsing CDDL.
    #[error("error while parsing CDDL: {0}")]
    ParsingError(String),

    /// Operation contains invalid values.
    #[error("invalid operation values")]
    ValidationError(String),

    /// `OperationFields` error.
    #[error("error while adding operation fields")]
    OperationFieldsError(#[from] crate::operation::OperationFieldsError),

    /// `Operation` error.
    #[error("error while creating operation")]
    OperationError(#[from] crate::operation::OperationError),
}

// This `Debug` implementation improves the display of error values from the `cddl` crate. Without
// this, all of its errors are concatenated into a long string that quickly becomes hard to read.
// By displaying cddl errors using `Display` instead of `Debug` below, we get line breaks in error
// messages. C.f. https://github.com/p2panda/p2panda/pull/207
impl std::fmt::Debug for CDDLValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            CDDLValidationError::InvalidSchema(_) => write!(f, "InvalidSchema"),
            CDDLValidationError::InvalidCBOR => write!(f, "InvalidCBOR"),
            CDDLValidationError::InvalidCDDL(_) => write!(f, "InvalidCDDL"),
            CDDLValidationError::NoSchema => write!(f, "NoSchema"),
            CDDLValidationError::ParsingError(_) => write!(f, "ParsingError"),
            CDDLValidationError::ValidationError(_) => write!(f, "ValidationError"),
            CDDLValidationError::OperationFieldsError(_) => write!(f, "OperationFieldsError"),
            CDDLValidationError::OperationError(_) => write!(f, "OperationError"),
        }?;

        // We want to format based on `Display` ("{}") instead of `Debug` ("{:?}") to respect line
        // breaks from the displayed error messages.
        f.write_str(format!("({})", self).as_ref())
    }
}
