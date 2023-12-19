// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::operation::header::error::DecodeHeaderError;
use crate::operation::header::Header;
use crate::operation::header::encoded_header::EncodedHeader;

pub fn decode_header(encoded_header: &EncodedHeader) -> Result<Header, DecodeHeaderError> {
    let bytes = encoded_header.to_bytes();

    let header: Header = ciborium::de::from_reader(&bytes[..]).map_err(|err| match err {
        ciborium::de::Error::Io(err) => DecodeHeaderError::DecoderIOFailed(err.to_string()),
        ciborium::de::Error::Syntax(pos) => DecodeHeaderError::InvalidCBOREncoding(pos),
        ciborium::de::Error::Semantic(_, err) => DecodeHeaderError::InvalidEncoding(err),
        ciborium::de::Error::RecursionLimitExceeded => DecodeHeaderError::RecursionLimitExceeded,
    })?;

    Ok(header)
}
