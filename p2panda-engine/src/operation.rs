// SPDX-License-Identifier: AGPL-3.0-or-later

use ciborium::de::Error as CiboriumError;
use p2panda_core::{Body, Header};
use serde::de::DeserializeOwned;
use thiserror::Error;

/// Encoded bytes of an operation header and optional body.
pub type RawOperation = (Vec<u8>, Option<Vec<u8>>);

/// Decodes operation header and optional body represented as CBOR bytes.
///
/// Fails when payload contains invalid encoding.
pub fn decode_operation<E>(
    header: &[u8],
    body: Option<&[u8]>,
) -> Result<(Header<E>, Option<Body>), DecodeError>
where
    E: DeserializeOwned,
{
    let header = ciborium::from_reader::<Header<E>, _>(header)
        .map_err(|err| Into::<DecodeError>::into(err))?;
    let body = body.map(Body::new);
    Ok((header, body))
}

#[derive(Debug, Error)]
pub enum DecodeError {
    /// An error occurred while reading bytes
    ///
    /// Contains the underlying error returned while reading.
    #[error("an error occurred while reading bytes: {0}")]
    Io(std::io::Error),

    /// An error occurred while parsing bytes
    ///
    /// Contains the offset into the stream where the syntax error occurred.
    #[error("an error occurred while parsing bytes at position {0}")]
    Syntax(usize),

    /// An error occurred while processing a parsed value
    ///
    /// Contains a description of the error that occurred and (optionally) the offset into the
    /// stream indicating the start of the item being processed when the error occurred.
    #[error("an error occurred while processing a parsed value at position {0:?}: {1}")]
    Semantic(Option<usize>, String),

    /// The input caused serde to recurse too much
    ///
    /// This error prevents a stack overflow.
    #[error("recursion limit exceeded while decoding")]
    RecursionLimitExceeded,
}

impl From<CiboriumError<std::io::Error>> for DecodeError {
    fn from(value: CiboriumError<std::io::Error>) -> Self {
        match value {
            CiboriumError::Io(err) => DecodeError::Io(err),
            CiboriumError::Syntax(offset) => DecodeError::Syntax(offset),
            CiboriumError::Semantic(offset, description) => {
                DecodeError::Semantic(offset, description)
            }
            CiboriumError::RecursionLimitExceeded => DecodeError::RecursionLimitExceeded,
        }
    }
}
