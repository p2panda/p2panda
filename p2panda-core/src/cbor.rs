// SPDX-License-Identifier: MIT OR Apache-2.0

//! Utility methods to encode or decode values in [CBOR] format.
//!
//! As per p2panda specification data-types like operation headers are encoded in the Concise
//! Binary Object Representation (CBOR) format.
//!
//! [CBOR]: https://cbor.io/
use std::io::Read;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Serializes a value into CBOR format.
pub fn encode_cbor<T: Serialize>(value: &T) -> Result<Vec<u8>, EncodeError> {
    let value = cbor_core::Value::serialized(&value)?;
    Ok(value.encode())
}

/// Deserializes a value which was formatted in CBOR.
pub fn decode_cbor<T: for<'a> Deserialize<'a>, R: Read>(reader: R) -> Result<T, DecodeError> {
    let value =
        cbor_core::Value::read_from(reader).map_err(|err| DecodeError::Io(Arc::new(err)))?;
    Ok(cbor_core::Value::deserialized(&value)?)
}

/// An error occurred during CBOR serialization.
#[derive(Debug, Error)]
#[error(transparent)]
pub struct EncodeError(#[from] cbor_core::SerdeError);

/// An error occurred during CBOR deserialization.
#[derive(Clone, Debug, Error)]
pub enum DecodeError {
    /// An error occurred while reading bytes.
    ///
    /// Contains the underlying error returned while reading.
    #[error("an error occurred while reading bytes: {0}")]
    Io(Arc<cbor_core::IoError>),

    #[error(transparent)]
    Serde(#[from] cbor_core::SerdeError),
}
