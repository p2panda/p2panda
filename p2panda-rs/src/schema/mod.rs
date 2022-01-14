// SPDX-License-Identifier: AGPL-3.0-or-later

//! Validations for operation payloads and definitions of system schemas.
//!
//! This uses [`Concise Data Definition Language`] (CDDL) internally to verify CBOR data of p2panda
//! operations.
//!
//! [`Concise Data Definition Language`]: https://tools.ietf.org/html/rfc8610
use cddl::validator::{cbor, validate_cbor_from_slice};
use wasm_bindgen::JsValue;

mod error;
mod operation;
#[allow(clippy::module_inception)]
mod schema;

pub use error::SchemaError;
pub use operation::OPERATION_SCHEMA;
pub use schema::{Schema, SchemaBuilder, Type};

/// Checks CBOR bytes against CDDL schemas.
///
/// This helper method also converts validation errors coming from the `cddl` crate into an
/// concatenated error operation and returns it.
#[cfg(not(target_arch = "wasm32"))]
pub fn validate_schema(cddl_schema: &str, bytes: Vec<u8>) -> Result<(), SchemaError> {
    match validate_cbor_from_slice(cddl_schema, &bytes, None) {
        Err(cbor::Error::Validation(err)) => {
            let err_str = err
                .iter()
                .map(|fe| format!("{}: \"{}\"", fe.cbor_location, fe.reason))
                .collect::<Vec<String>>()
                .join(", ");

            Err(error::SchemaError::InvalidSchema(err_str))
        }
        Err(cbor::Error::CBORParsing(_err)) => Err(error::SchemaError::InvalidCBOR),
        Err(cbor::Error::CDDLParsing(err)) => {
            panic!("Parsing CDDL error: {}", err);
        }
        _ => Ok(()),
    }
}

/// Checks CBOR bytes against CDDL schemas.
///
/// This helper method also converts validation errors coming from the `cddl` crate into an
/// concatenated error operation and returns it.
#[cfg(target_arch = "wasm32")]
pub fn validate_schema(cddl_schema: &str, bytes: Vec<u8>) -> Result<(), JsValue> {
    validate_cbor_from_slice(cddl_schema, &bytes, None)?;
    Ok(())
}
