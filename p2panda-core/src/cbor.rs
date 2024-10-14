// SPDX-License-Identifier: AGPL-3.0-or-later

use ciborium::de::Error as DeserializeError;
use ciborium::ser::Error as SerializeError;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::Header;

pub fn encode_header<E: Serialize>(header: &Header<E>) -> Result<Vec<u8>, EncodeError> {
    let mut bytes = Vec::new();
    ciborium::ser::into_writer(header, &mut bytes).map_err(Into::<EncodeError>::into)?;
    Ok(bytes)
}

pub fn decode_header<E: for<'a> Deserialize<'a>>(bytes: &[u8]) -> Result<Header<E>, DecodeError> {
    let header = ciborium::from_reader::<Header<E>, _>(bytes).map_err(Into::<DecodeError>::into)?;
    Ok(header)
}

#[derive(Debug, Error)]
pub enum EncodeError {
    /// An error occurred while writing bytes.
    ///
    /// Contains the underlying error returned while reading.
    #[error("an error occurred while reading bytes: {0}")]
    Io(std::io::Error),

    /// An error indicating a value that cannot be serialized.
    ///
    /// Contains a description of the problem delivered from serde.
    #[error("an error occurred while deserializing value: {0}")]
    Value(String),
}

impl From<SerializeError<std::io::Error>> for EncodeError {
    fn from(value: SerializeError<std::io::Error>) -> Self {
        match value {
            SerializeError::Io(err) => EncodeError::Io(err),
            SerializeError::Value(err) => EncodeError::Value(err),
        }
    }
}

#[derive(Debug, Error)]
pub enum DecodeError {
    /// An error occurred while reading bytes.
    ///
    /// Contains the underlying error returned while reading.
    #[error("an error occurred while reading bytes: {0}")]
    Io(std::io::Error),

    /// An error occurred while parsing bytes.
    ///
    /// Contains the offset into the stream where the syntax error occurred.
    #[error("an error occurred while parsing bytes at position {0}")]
    Syntax(usize),

    /// An error occurred while processing a parsed value.
    ///
    /// Contains a description of the error that occurred and (optionally) the offset into the
    /// stream indicating the start of the item being processed when the error occurred.
    #[error("an error occurred while processing a parsed value at position {0:?}: {1}")]
    Semantic(Option<usize>, String),

    /// The input caused serde to recurse too much.
    ///
    /// This error prevents a stack overflow.
    #[error("recursion limit exceeded while decoding")]
    RecursionLimitExceeded,
}

impl From<DeserializeError<std::io::Error>> for DecodeError {
    fn from(value: DeserializeError<std::io::Error>) -> Self {
        match value {
            DeserializeError::Io(err) => DecodeError::Io(err),
            DeserializeError::Syntax(offset) => DecodeError::Syntax(offset),
            DeserializeError::Semantic(offset, description) => {
                DecodeError::Semantic(offset, description)
            }
            DeserializeError::RecursionLimitExceeded => DecodeError::RecursionLimitExceeded,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::extensions::DefaultExtensions;
    use crate::{Body, Header, PrivateKey};

    use super::{decode_header, encode_header};

    #[test]
    fn encode_decode() {
        let private_key = PrivateKey::new();
        let body = Body::new(&[1, 2, 3]);
        let mut header = Header::<DefaultExtensions> {
            public_key: private_key.public_key(),
            payload_size: body.size(),
            payload_hash: Some(body.hash()),
            ..Default::default()
        };
        header.sign(&private_key);

        let bytes = encode_header(&header).unwrap();
        let header_again: Header<DefaultExtensions> = decode_header(&bytes).unwrap();

        assert_eq!(header.hash(), header_again.hash());
    }
}
