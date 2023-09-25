// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::operation_v2::body::error::DecodeBodyError;
use crate::operation_v2::body::plain::PlainOperation;
use crate::operation_v2::body::EncodedBody;

pub fn decode_body(encoded_body: &EncodedBody) -> Result<PlainOperation, DecodeBodyError> {
    let bytes = encoded_body.into_bytes();

    let body: PlainOperation = ciborium::de::from_reader(&bytes[..]).map_err(|err| match err {
        ciborium::de::Error::Io(err) => DecodeBodyError::DecoderIOFailed(err.to_string()),
        ciborium::de::Error::Syntax(pos) => DecodeBodyError::InvalidCBOREncoding(pos),
        ciborium::de::Error::Semantic(_, err) => DecodeBodyError::InvalidEncoding(err),
        ciborium::de::Error::RecursionLimitExceeded => DecodeBodyError::RecursionLimitExceeded,
    })?;

    Ok(body)
}
