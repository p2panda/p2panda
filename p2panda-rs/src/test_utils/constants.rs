// SPDX-License-Identifier: AGPL-3.0-or-later

//! Constants used across the test_utils module for default values.

use crate::operation::OperationValue;

/// The default hash string, used when a hash is needed for testing, it's the default hash in
/// fixtures when a custom value isn't specified.
pub const DEFAULT_HASH: &str =
    "0020b177ec1bf26dfb3b7010d473e6d44713b29b765b99c6e60ecbfae742de496543";

/// Unpublished test schema id, used in all operation fixtures that don't define their own schema.
pub const TEST_SCHEMA_ID: &str =
    "venue_0020c65567ae37efea293e34a9c7d13f8f2bf23dbdc3b5c7b9ab46293111c48fc78b";

/// The default private key string, used for creating authors and public keys in fixtures.
pub const DEFAULT_PRIVATE_KEY: &str =
    "eb852fefa703901e42f17cdc2aa507947f392a72101b2c1a6d30023af14f75e2";

/// The sequence number of entries containing skiplinks up to seq_num = 20.
pub const SKIPLINK_ENTRIES: [u64; 5] = [4, 8, 12, 13, 17];

/// The default sequence number, used when an entry is created in a fixture and no custom values
/// are provided.
pub const DEFAULT_SEQ_NUM: u64 = 1;

pub fn default_fields() -> Vec<(&'static str, OperationValue)> {
    [
        ("username", OperationValue::Text("bubu".to_owned())),
        ("age", OperationValue::Integer(28)),
        ("height", OperationValue::Float(3.5)),
        ("is_admin", OperationValue::Boolean(false)),
        (
            "profile_picture",
            OperationValue::Relation(crate::operation::Relation::new(
                "0020b177ec1bf26dfb3b7010d473e6d44713b29b765b99c6e60ecbfae742de496543"
                    .parse()
                    .unwrap(),
            )),
        ),
        (
            "my_friends",
            OperationValue::RelationList(crate::operation::RelationList::new(vec![
                "0020b177ec1bf26dfb3b7010d473e6d44713b29b765b99c6e60ecbfae742de496543"
                    .parse()
                    .unwrap(),
            ])),
        ),
    ]
    .to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hash::Hash;
    use crate::operation::OperationId;
    use crate::schema::SchemaId;

    #[test]
    fn default_hash() {
        let default_hash = Hash::new_from_bytes(vec![1, 2, 3]).unwrap();
        assert_eq!(default_hash.as_str(), DEFAULT_HASH)
    }

    #[test]
    fn default_schema() {
        let venue_schema_hash: OperationId = Hash::new_from_bytes(vec![3, 2, 1]).unwrap().into();
        let schema = SchemaId::new_application("venue", &venue_schema_hash.into());
        assert_eq!(schema.as_str(), TEST_SCHEMA_ID)
    }
}
