// SPDX-License-Identifier: AGPL-3.0-or-later

//! Validations for message payloads and definitions of system schemas.
//!
//! This uses [`Concise Data Definition Language`] (CDDL) internally to verify CBOR data of p2panda
//! messages.
//!
//! [`Concise Data Definition Language`]: https://tools.ietf.org/html/rfc8610
#[cfg(not(target_arch = "wasm32"))]
use cddl::validator::cbor;

mod message;
mod schema;
mod utils;
mod error;

pub use message::MESSAGE_SCHEMA;
pub use utils::{USER_SCHEMA, USER_SCHEMA_HASH};
pub use schema::{Type, Schema, SchemaBuilder};
pub use error::SchemaError;

/// Checks CBOR bytes against CDDL schemas.
///
/// This helper method also converts validation errors coming from the cddl crate into an
/// concatenated error message and returns it.
#[cfg(not(target_arch = "wasm32"))]
pub fn validate_schema(cddl_schema: &str, bytes: Vec<u8>) -> Result<(), SchemaError> {
    match cddl::validate_cbor_from_slice(cddl_schema, &bytes) {
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
