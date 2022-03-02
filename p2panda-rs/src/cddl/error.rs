// SPDX-License-Identifier: AGPL-3.0-or-later

use thiserror::Error;

/// Error types for schema validation.
#[derive(Error)]
pub enum CDDLValidationError {
    /// Operation contains invalid cbor data.
    // Note: We pretty-print the vector of error strings to get line breaks
    #[error("invalid operation cbor format: {0:#?}")]
    InvalidCDDL(Vec<String>),

    /// Operation can't be deserialised from invalid CBOR encoding.
    #[error("invalid CBOR format")]
    ParsingCBOR,

    /// Attempted to validate an operation using an invalid CDDL definition
    #[error("invalid CDDL definition: {0}")]
    ParsingCDDL(String),
}

// This `Debug` implementation improves the display of error values from the `cddl` crate. Without
// this, all of its errors are concatenated into a long string that quickly becomes hard to read.
// By displaying cddl errors using `Display` instead of `Debug` below, we get line breaks in error
// messages. C.f. https://github.com/p2panda/p2panda/pull/207
impl std::fmt::Debug for CDDLValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            CDDLValidationError::InvalidCDDL(_) => write!(f, "InvalidCDDL"),
            CDDLValidationError::ParsingCBOR => write!(f, "ParsingCBOR"),
            CDDLValidationError::ParsingCDDL(_) => write!(f, "ParsingCDDL"),
        }?;

        // We want to format based on `Display` ("{}") instead of `Debug` ("{:?}") to respect line
        // breaks from the displayed error messages.
        f.write_str(format!("({})", self).as_ref())
    }
}
