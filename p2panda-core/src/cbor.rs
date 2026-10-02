// SPDX-License-Identifier: MIT OR Apache-2.0

//! Utility methods to encode or decode values in [CBOR] format.
//!
//! As per p2panda specification data-types like operation headers are encoded in the Concise
//! Binary Object Representation (CBOR) format.
//!
//! [CBOR]: https://cbor.io/
use std::io::Read;
use std::sync::Arc;

use cbor_core::DecodeOptions;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Serializes a value into CBOR format.
pub fn encode_cbor<T: Serialize>(value: &T) -> Result<Vec<u8>, EncodeError> {
    let value = cbor_core::Value::serialized(&value)?;
    Ok(value.encode())
}

/// Deserializes a value which was formatted in CBOR.
///
/// ## Robustness principle
///
/// > "be conservative in what you send, be liberal in what you accept". (Postel's law)
///
/// This uses a less strict CBOR decoder and should only be paired with data which doesn't need to
/// have deterministic, canonical byte encoding, for example data which is dumped into stores will
/// not benefit much from a strict decoder.
///
/// See [`decode_cbor_strict`] for a strict CBOR decoder variant.
pub fn decode_cbor<T: for<'a> Deserialize<'a>, R: Read>(reader: R) -> Result<T, DecodeError> {
    let value = DecodeOptions::default()
        .strictness(cbor_core::Strictness::LENIENT)
        .read_from(reader)
        .map_err(|err| DecodeError::Io(Arc::new(err)))?;
    Ok(cbor_core::Value::deserialized(&value)?)
}

/// Deserializes a value which was formatted in canonical (strict) CBOR.
///
/// This will fail if otherwise correct CBOR was encoded non-canonically. This is useful for
/// data-types which rely on determinstic hash digests and signatures.
///
/// See CBOR Core specification: <https://www.ietf.org/archive/id/draft-rundgren-cbor-core-25.html>.
pub fn decode_cbor_strict<T: for<'a> Deserialize<'a>, R: Read>(
    reader: R,
) -> Result<T, DecodeError> {
    let value =
        cbor_core::Value::read_from(reader).map_err(|err| DecodeError::Io(Arc::new(err)))?;
    Ok(cbor_core::Value::deserialized(&value)?)
}

/// An error occurred during CBOR serialization.
#[derive(Debug, Error)]
#[error(transparent)]
pub struct EncodeError(#[from] cbor_core::SerdeError);

/// An error occurred during CBOR deserialization.
#[derive(Clone, Debug, Error)]
pub enum DecodeError {
    /// An error occurred while reading bytes.
    ///
    /// Contains the underlying error returned while reading.
    #[error("an error occurred while reading bytes: {0}")]
    Io(Arc<cbor_core::IoError>),

    #[error(transparent)]
    Serde(#[from] cbor_core::SerdeError),
}

#[cfg(test)]
mod tests {
    use cbor_core::{Value, array, map};

    use super::{decode_cbor, decode_cbor_strict};

    #[test]
    fn lenient_cbor_decoding() {
        let value = map! {
            2 => array![10, 20, 30],
            1 => "hello",
        };

        // cbor_core will sort the keys "1" and "2" in lexicographical order (canonical encoding).
        let hex = value.encode_hex();

        // This is valid CBOR, but in non-lexicographical order (key 2 comes first).
        let non_canonical_hex = "a202830a14181e016568656c6c6f".to_string();
        //                         ^^          ^^

        assert!(hex != non_canonical_hex, "hex encodings are not the same");

        let non_canonical_bytes = hex::decode(non_canonical_hex).unwrap();

        assert!(
            Value::decode_hex(hex).is_ok(),
            "decoding canonical encoding is valid"
        );
        assert!(
            decode_cbor_strict::<Value, _>(&non_canonical_bytes[..]).is_err(),
            "decoding non-canonical fails by default"
        );
        assert!(
            decode_cbor::<Value, _>(&non_canonical_bytes[..]).is_ok(),
            "decoding non-canonical doesn't fail when using decode_cbor fn"
        );
    }
}
