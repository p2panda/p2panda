// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::next::operation::error::EncodeOperationError;
use crate::next::operation::plain::PlainOperation;
use crate::next::operation::{EncodedOperation, Operation};

pub fn encode_operation(operation: &Operation) -> Result<EncodedOperation, EncodeOperationError> {
    // Convert to plain operation format
    let plain: PlainOperation = operation.into();

    // Encode as CBOR byte sequence
    let encoded_operation = encode_plain_operation(&plain)?;

    Ok(encoded_operation)
}

pub fn encode_plain_operation(
    plain: &PlainOperation,
) -> Result<EncodedOperation, EncodeOperationError> {
    let mut cbor_bytes = Vec::new();

    ciborium::ser::into_writer(&plain, &mut cbor_bytes).map_err(|err| match err {
        ciborium::ser::Error::Io(err) => EncodeOperationError::EncoderIOFailed(err.to_string()),
        ciborium::ser::Error::Value(err) => EncodeOperationError::EncoderFailed(err.to_string()),
    })?;

    Ok(EncodedOperation(cbor_bytes))
}
