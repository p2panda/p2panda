// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::operation_v2::header::error::DecodeHeaderError;
use crate::operation_v2::header::traits::AsEncodedHeader;
use crate::operation_v2::header::Header;

pub fn decode_header(encoded_header: &impl AsEncodedHeader) -> Result<Header, DecodeHeaderError> {
    let bytes = encoded_header.to_bytes();

    let header: Header = ciborium::de::from_reader(&bytes[..]).map_err(|err| match err {
        ciborium::de::Error::Io(err) => DecodeHeaderError::DecoderIOFailed(err.to_string()),
        ciborium::de::Error::Syntax(pos) => DecodeHeaderError::InvalidCBOREncoding(pos),
        ciborium::de::Error::Semantic(_, err) => DecodeHeaderError::InvalidEncoding(err),
        ciborium::de::Error::RecursionLimitExceeded => DecodeHeaderError::RecursionLimitExceeded,
    })?;

    Ok(header)
}
