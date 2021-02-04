#[cfg(not(target_arch = "wasm32"))]
use anyhow::bail;
#[cfg(not(target_arch = "wasm32"))]
use cddl::validator::cbor;

#[cfg(not(target_arch = "wasm32"))]
use crate::Result;

mod message;

pub use message::MESSAGE_SCHEMA;

/// Custom error types of schema validation.
pub mod error {
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
    }
}

/// Checks CBOR bytes against CDDL schemas.
///
/// This helper method also converts validation errors coming from the cddl crate into an
/// concatenated error message and returns it.
#[cfg(not(target_arch = "wasm32"))]
pub fn validate_schema(cddl_schema: &str, bytes: Vec<u8>) -> Result<()> {
    match cddl::validate_cbor_from_slice(cddl_schema, &bytes) {
        Err(cbor::Error::Validation(err)) => {
            let err_str = err
                .iter()
                .map(|fe| format!("{}: \"{}\"", fe.cbor_location, fe.reason))
                .collect::<Vec<String>>()
                .join(", ");

            bail!(error::SchemaError::InvalidSchema(err_str))
        }
        Err(cbor::Error::CBORParsing(_err)) => bail!(error::SchemaError::InvalidCBOR),
        Err(cbor::Error::CDDLParsing(err)) => {
            panic!(err);
        }
        _ => Ok(()),
    }
}
