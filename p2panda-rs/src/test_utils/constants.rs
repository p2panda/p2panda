// SPDX-License-Identifier: AGPL-3.0-or-later

//! Constants used across the test_utils module for default values.

/// The default hash string, used when a hash is needed for testing, it's the default hash in
/// fixtures when a custom value isn't specified.
pub const DEFAULT_HASH: &str =
    "0020b177ec1bf26dfb3b7010d473e6d44713b29b765b99c6e60ecbfae742de496543";

/// The default schema hash string, used in all operation fixtures when no custom schema hash is
/// defined.
pub const DEFAULT_SCHEMA_HASH: &str =
    "0020c65567ae37efea293e34a9c7d13f8f2bf23dbdc3b5c7b9ab46293111c48fc78b";

/// The default private key string, used for creating authors and public keys in fixtures.
pub const DEFAULT_PRIVATE_KEY: &str =
    "eb852fefa703901e42f17cdc2aa507947f392a72101b2c1a6d30023af14f75e2";

/// The default sequence number, used when an entry is created in a fixture and no custom values
/// are provided.
pub const DEFAULT_SEQ_NUM: i64 = 1;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hash::Hash;

    #[test]
    fn default_hash() {
        let default_hash = Hash::new_from_bytes(vec![1, 2, 3]).unwrap();
        assert_eq!(default_hash.as_str(), DEFAULT_HASH)
    }

    #[test]
    fn default_schema() {
        let default_schema_hash = Hash::new_from_bytes(vec![3, 2, 1]).unwrap();
        assert_eq!(default_schema_hash.as_str(), DEFAULT_SCHEMA_HASH)
    }
}
