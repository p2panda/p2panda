// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::identity::KeyPair;
use crate::operation::OperationVersion;
use crate::operation::body::EncodedBody;
use crate::operation::header::error::EncodeHeaderError;
use crate::operation::header::{EncodedHeader, Header, HeaderExtension};

pub fn sign_header(
    extension: HeaderExtension,
    payload: &EncodedBody,
    key_pair: &KeyPair,
) -> Result<Header, EncodeHeaderError> {
    let mut header = Header(
        OperationVersion::V1,
        key_pair.public_key(),
        payload.hash(),
        payload.size(),
        extension,
        None,
    );

    let unsigned_bytes = encode_header(&header)?.unsigned_bytes();
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
    extension: HeaderExtension,
    payload: &EncodedBody,
    key_pair: &KeyPair,
) -> Result<EncodedHeader, EncodeHeaderError> {
    let header = sign_header(extension, payload, key_pair)?;
    let encoded_header = encode_header(&header)?;
    Ok(encoded_header)
}