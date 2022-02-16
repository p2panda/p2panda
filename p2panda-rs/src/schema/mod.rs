// SPDX-License-Identifier: AGPL-3.0-or-later

//! Validations for operation payloads and definitions of system schemas.
//!
//! This uses [`Concise Data Definition Language`] (CDDL) internally to verify CBOR data of p2panda
//! operations.
//!
//! [`Concise Data Definition Language`]: https://tools.ietf.org/html/rfc8610
#[cfg(not(target_arch = "wasm32"))]
use cddl::validator::cbor;

mod error;
mod operation;
#[allow(clippy::module_inception)]
mod schema;
mod schema_type;

pub use error::SchemaError;
pub use operation::OPERATION_SCHEMA;
pub use schema::{Schema, SchemaBuilder, Type, ValidateOperation};
pub use schema_type::SchemaType;

/// Checks CBOR bytes against CDDL schemas.
///
/// This helper method also converts validation errors coming from the `cddl` crate into an
/// concatenated error operation and returns it.
#[cfg(not(target_arch = "wasm32"))]
pub fn validate_schema(cddl_schema: &str, bytes: Vec<u8>) -> Result<(), SchemaError> {
    match cddl::validate_cbor_from_slice(cddl_schema, &bytes) {
        Err(cbor::Error::Validation(err)) => {
            let err_str = err
                .iter()
                .map(|fe| {
                    format!("{}", fe)
                        // Quotes escaped in error messages from `cddl` crate are actually not unescaped by
                        // format macro.
                        //
                        // See: https://github.com/anweiss/cddl/blob/main/src/validator/cbor.rs#L100
                        .replace("\"", "'")
                })
                .collect::<Vec<String>>();

            Err(error::SchemaError::InvalidSchema(err_str))
        }
        Err(cbor::Error::CBORParsing(_err)) => Err(error::SchemaError::InvalidCBOR),
        Err(cbor::Error::CDDLParsing(err)) => {
            panic!("Parsing CDDL error: {}", err);
        }
        _ => Ok(()),
    }
}
