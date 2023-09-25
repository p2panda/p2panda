// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::entry::traits::AsEncodedEntry;
use crate::identity_v2::KeyPair;
use crate::operation_v2::body::EncodedBody;
use crate::operation_v2::header::error::EncodeHeaderError;
use crate::operation_v2::header::{EncodedHeader, Header, HeaderExtension, HeaderVersion};

pub fn sign_header(
    extension: &HeaderExtension,
    payload: &EncodedBody,
    key_pair: &KeyPair,
) -> Result<Header, EncodeHeaderError> {
    // Calculate payload hash and size from payload
    let payload_hash = payload.hash();
    let payload_size = payload.size();

    // Prepare header without any signature yet
    let mut header = Header(
        HeaderVersion::V1,
        key_pair.public_key(),
        payload_hash,
        payload_size,
        extension.to_owned(),
        None,
    );

    // Get unsigned header bytes
    let unsigned_bytes = encode_header(&header)?.unsigned_bytes();

    // Sign and store signature in header
    header.5 = Some(key_pair.sign(&unsigned_bytes));

    Ok(header)
}

pub fn encode_header(header: &Header) -> Result<EncodedHeader, EncodeHeaderError> {
    let mut cbor_bytes = Vec::new();

    ciborium::ser::into_writer(&header, &mut cbor_bytes).map_err(|err| match err {
        ciborium::ser::Error::Io(err) => EncodeHeaderError::EncoderIOFailed(err.to_string()),
        ciborium::ser::Error::Value(err) => EncodeHeaderError::EncoderFailed(err),
    })?;

    Ok(EncodedHeader::from_bytes(&cbor_bytes))
}

pub fn sign_and_encode_entry(
    extension: &HeaderExtension,
    payload: &EncodedBody,
    key_pair: &KeyPair,
) -> Result<EncodedHeader, EncodeHeaderError> {
    let header = sign_header(extension, payload, key_pair)?;
    let encoded_header = encode_header(&header)?;
    Ok(encoded_header)
}
