// SPDX-License-Identifier: AGPL-3.0-or-later

//! Constants used across the `test_utils` module for default values.

use std::convert::TryFrom;
use std::str::FromStr;

use crate::document::{DocumentId, DocumentViewId};
use crate::operation::OperationValue;
use crate::schema::{FieldType, Schema};
use crate::test_utils::fixtures::{schema_fields, schema_id};

/// Hash value, used when a hash is needed for testing. It's the default hash in fixtures
/// when a custom value isn't specified.
pub const HASH: &str = "b177ec1bf26dfb3b7010d473e6d44713b29b765b99c6e60ecbfae742de496543";

/// Schema id, used as the default in all fixtures.
pub const SCHEMA_ID: &str =
    "venue_c65567ae37efea293e34a9c7d13f8f2bf23dbdc3b5c7b9ab46293111c48fc78b";

/// Private key string, used as default for creating private keys in fixtures.
pub const PRIVATE_KEY: &str = "eb852fefa703901e42f17cdc2aa507947f392a72101b2c1a6d30023af14f75e2";

/// Sequence number of entries containing skiplinks up to seq_num = 20.
pub const SKIPLINK_SEQ_NUMS: [u64; 5] = [4, 8, 12, 13, 17];

/// Constant operation fields used throughout the fixtures module.
pub fn test_fields() -> Vec<(&'static str, OperationValue)> {
    [
        ("age", 28.into()),
        (
            "comments",
            vec![
                DocumentViewId::try_from(vec![
                    "4f0dd3a1b8205b6d4ce3fd4c158bb91c9e131bd842e727164ea220b5b6d09346",
                ])
                .unwrap(),
                DocumentViewId::try_from(vec![
                    "19ed3e9b39cd17f1dbc0f6e31a6e7b9c9ab7e349332e710c946a441b7d308eb5",
                    "995d53f460293c5686c42037b72787ed28668ad8b6d18e9d5f02c5d3301161f0",
                ])
                .unwrap(),
            ]
            .into(),
        ),
        ("height", 3.5.into()),
        ("is_admin", false.into()),
        (
            "my_friends",
            vec![DocumentId::from_str(
                "9a2149589672fa1ac2348e48b4c56fc208a0eff44938464dd2091850f444a323",
            )
            .unwrap()]
            .into(),
        ),
        (
            "past_event",
            DocumentViewId::try_from(vec![
                "475488c0e2bbb9f5a81929e2fe11de81c1f83c8045de41da43899d25ad0d4afa",
                "f7a17e14b9a5e87435decdbc28d562662fbf37da39b94e8469d8e1873336e80e",
            ])
            .unwrap()
            .into(),
        ),
        (
            "profile_picture",
            DocumentId::from_str(
                "b177ec1bf26dfb3b7010d473e6d44713b29b765b99c6e60ecbfae742de496543",
            )
            .unwrap()
            .into(),
        ),
        ("username", "bubu".into()),
        ("data", vec![0, 1, 2, 3].into()),
    ]
    .to_vec()
}

/// Constant Schema used throughout the fixtures module.
///
/// Derived from the test fields defined above.
pub fn schema() -> Schema {
    let id = schema_id(SCHEMA_ID);
    let fields = schema_fields(test_fields(), id.clone());
    let fields: Vec<(&str, FieldType)> = fields
        .iter()
        .map(|(name, field_type)| (name.as_str(), field_type.to_owned()))
        .collect();

    Schema::new(&id, "Test schema", &fields).unwrap()
}

#[cfg(test)]
mod tests {
    use crate::document::DocumentViewId;
    use crate::hash::Hash;
    use crate::operation::OperationId;
    use crate::schema::{SchemaId, SchemaName};

    use super::*;

    #[test]
    fn default_hash() {
        let default_hash = Hash::new_from_bytes(&[1, 2, 3]);
        assert_eq!(default_hash.as_str(), HASH)
    }

    #[test]
    fn default_schema() {
        let venue_operation_id: OperationId = Hash::new_from_bytes(&[3, 2, 1]).into();
        let venue_schema_name = SchemaName::new("venue").expect("Valid schema name");
        let schema = SchemaId::new_application(
            &venue_schema_name,
            &DocumentViewId::new(&[venue_operation_id]),
        );
        assert_eq!(schema.to_string(), SCHEMA_ID)
    }
}
