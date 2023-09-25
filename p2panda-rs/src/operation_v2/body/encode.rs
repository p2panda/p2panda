// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::operation_v2::body::error::EncodeBodyError;
use crate::operation_v2::body::plain::PlainOperation;
use crate::operation_v2::body::{Body, EncodedBody};

pub fn encode_body(body: &Body) -> Result<EncodedBody, EncodeBodyError> {
    let plain: PlainOperation = body.into();
    let encoded_body = encode_plain_body(&plain)?;
    Ok(encoded_body)
}

pub fn encode_plain_body(plain: &PlainOperation) -> Result<EncodedBody, EncodeBodyError> {
    let mut cbor_bytes = Vec::new();

    ciborium::ser::into_writer(&plain, &mut cbor_bytes).map_err(|err| match err {
        ciborium::ser::Error::Io(err) => EncodeBodyError::EncoderIOFailed(err.to_string()),
        ciborium::ser::Error::Value(err) => EncodeBodyError::EncoderFailed(err),
    })?;

    Ok(EncodedBody::from_bytes(&cbor_bytes))
}
