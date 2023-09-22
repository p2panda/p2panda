// SPDX-License-Identifier: AGPL-3.0-or-later

use std::fmt::Display;
use std::hash::Hash as StdHash;

use serde::{Deserialize, Serialize};

use crate::hash_v2::Hash;
use crate::serde::{deserialize_hex, serialize_hex};

/// Wrapper type for operation bytes.
///
/// This struct can be used to deserialize an hex-encoded string into bytes when using a
/// human-readable encoding format. No validation is applied whatsoever, except of checking if it
/// is a valid hex-string (#OP1).
///
/// To validate these bytes use the `decode_operation` method to get an `PlainOperation` instance.
/// From there you can derive a `Schema` to finally validate the operation with
/// `validate_operation`. Read the module-level documentation for more information.
#[derive(Clone, Debug, PartialEq, Eq, StdHash, Serialize, Deserialize)]
pub struct EncodedBody(
    #[serde(serialize_with = "serialize_hex", deserialize_with = "deserialize_hex")] Vec<u8>,
);

impl EncodedBody {
    /// Returns new `EncodedBody` instance from given bytes.
    ///
    /// This does not apply any validation and should only be used in methods where all checks have
    /// taken place before.
    pub fn from_bytes(bytes: &[u8]) -> Self {
        Self(bytes.to_owned())
    }

    /// Returns the hash of this operation.
    pub fn hash(&self) -> Hash {
        Hash::new_from_bytes(&self.0)
    }

    /// Returns operation as bytes.
    pub fn into_bytes(&self) -> Vec<u8> {
        self.0.clone()
    }

    /// Returns payload size (number of bytes) of encoded operation.
    pub fn size(&self) -> u64 {
        self.0.len() as u64
    }
}

impl Display for EncodedBody {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", hex::encode(&self.0))
    }
}

#[cfg(any(feature = "test-utils", test))]
impl EncodedBody {
    /// Returns a new instance of `EncodedBody` for testing.
    pub fn new(bytes: &[u8]) -> EncodedBody {
        Self(bytes.to_owned())
    }

    /// Converts hexadecimal string into bytes and returns as a new instance of `EncodedBody`.
    pub fn from_hex(value: &str) -> EncodedBody {
        let bytes = hex::decode(value).expect("invalid hexadecimal value");
        Self(bytes)
    }
}

#[cfg(test)]
mod tests {
    use ciborium::cbor;

    use crate::operation_v2::body::EncodedBody;
    use crate::serde::serialize_value;

    #[test]
    fn byte_and_str_representation() {
        let bytes = serialize_value(cbor!([
            1,
            2,
            "schema_field_definition_v1",
            ["00200f64117685c68c82154fb87260e670884a410a4add69c33bf4f606297b83b6ca"]
        ]));

        let encoded_operation = EncodedBody::from_bytes(&bytes);

        // Return bytes and size
        assert_eq!(encoded_operation.into_bytes(), bytes);
        assert_eq!(encoded_operation.size() as usize, bytes.len());

        // Represent bytes as hexadecimal string
        assert_eq!(
            concat!(
                "840102781a736368656d615f6669656c645f646566696e69746",
                "96f6e5f76318178443030323030663634313137363835633638",
                "633832313534666238373236306536373038383461343130613",
                "461646436396333336266346636303632393762383362366361"
            ),
            format!("{}", encoded_operation)
        );
    }

    /*#[rstest]
    fn it_hashes(encoded_body: EncodedBody) {
        // Use operation as key in hash map
        let mut hash_map = HashMap::new();
        let key_value = "Value identified by a hash".to_string();
        hash_map.insert(&encoded_body, key_value.clone());

        // Retreive value from hash map via key
        let key_value_retrieved = hash_map.get(&encoded_operation).unwrap().to_owned();
        assert_eq!(key_value, key_value_retrieved)
    }*/
}
