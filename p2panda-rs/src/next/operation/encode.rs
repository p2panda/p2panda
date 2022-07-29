// SPDX-License-Identifier: AGPL-3.0-or-later

//! Methods to encode operations.
//!
//! Encoding an operation does not require a schema, the `Operation` will be serialized into bytes,
//! represented as a `EncodedOperation` which is then ready to be sent to a p2panda node (alongside
//! an entry).
//!
//! ```text
//! ┌─────────┐                           ┌────────────────┐
//! │Operation│ ───encode_operation()───► │EncodedOperation│ ────► bytes
//! └─────────┘                           └────────────────┘
//! ```
use crate::next::operation::error::EncodeOperationError;
use crate::next::operation::plain::PlainOperation;
use crate::next::operation::{EncodedOperation, Operation};

/// Encodes an operation in canonic format.
pub fn encode_operation(operation: &Operation) -> Result<EncodedOperation, EncodeOperationError> {
    // Convert to plain operation format
    let plain: PlainOperation = operation.into();

    // Encode as CBOR byte sequence
    let encoded_operation = encode_plain_operation(&plain)?;

    Ok(encoded_operation)
}

/// Encodes a `PlainOperation` instance in canonic format.
pub fn encode_plain_operation(
    plain: &PlainOperation,
) -> Result<EncodedOperation, EncodeOperationError> {
    let mut cbor_bytes = Vec::new();

    ciborium::ser::into_writer(&plain, &mut cbor_bytes).map_err(|err| match err {
        ciborium::ser::Error::Io(err) => EncodeOperationError::EncoderIOFailed(err.to_string()),
        ciborium::ser::Error::Value(err) => EncodeOperationError::EncoderFailed(err),
    })?;

    Ok(EncodedOperation::from_bytes(&cbor_bytes))
}
