// SPDX-License-Identifier: AGPL-3.0-or-later

use std::fmt::Display;
use std::hash::Hash as StdHash;

use serde::{Deserialize, Serialize};

use crate::hash::Hash;
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
pub struct EncodedOperation(
    #[serde(serialize_with = "serialize_hex", deserialize_with = "deserialize_hex")] Vec<u8>,
);

impl EncodedOperation {
    /// Returns new `EncodedOperation` instance from given bytes.
    ///
    /// This does not apply any validation and should only be used in methods where all checks have
    /// taken place before.
    pub(crate) fn from_bytes(bytes: &[u8]) -> Self {
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

impl Display for EncodedOperation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", hex::encode(&self.0))
    }
}

#[cfg(any(feature = "testing", test))]
impl EncodedOperation {
    pub fn new(bytes: &[u8]) -> EncodedOperation {
        Self(bytes.to_owned())
    }

    pub fn from_str(value: &str) -> EncodedOperation {
        let bytes = hex::decode(value).expect("invalid hexadecimal value");
        Self(bytes)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use rstest::rstest;

    use crate::operation::EncodedOperation;
    use crate::test_utils::fixtures::encoded_operation;

    #[rstest]
    fn it_hashes(encoded_operation: EncodedOperation) {
        // Use operation as key in hash map
        let mut hash_map = HashMap::new();
        let key_value = "Value identified by a hash".to_string();
        hash_map.insert(&encoded_operation, key_value.clone());

        // Retreive value from hash map via key
        let key_value_retrieved = hash_map.get(&encoded_operation).unwrap().to_owned();
        assert_eq!(key_value, key_value_retrieved)
    }
}
