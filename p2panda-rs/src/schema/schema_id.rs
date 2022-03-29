// SPDX-License-Identifier: AGPL-3.0-or-later

use std::fmt;
use std::str::FromStr;

use serde::de::Visitor;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use yasmf_hash::MAX_YAMF_HASH_SIZE;

use crate::document::DocumentViewId;
use crate::operation::OperationId;
use crate::schema::error::SchemaIdError;

/// Identifies the schema of an [`crate::operation::Operation`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SchemaId {
    /// An application schema.
    Application(String, DocumentViewId),

    /// A schema definition.
    Schema,

    /// A schema definition field.
    SchemaField,

    /// A key group.
    KeyGroup,

    /// A key group membership request.
    KeyGroupRequest,

    /// A key group membership response.
    KeyGroupResponse,
}

impl SchemaId {
    /// Instantiate a new `SchemaId` from a hash string or system schema name.
    ///
    /// If a hash string is passed, it will be converted into a document view id with only one hash
    /// inside.
    pub fn new(id: &str) -> Result<Self, SchemaIdError> {
        match SchemaId::parse_system_schema_id(id) {
            Some(value) => Ok(value),
            None => Self::parse_application_schema_str(id),
        }
    }

    fn parse_system_schema_id(id: &str) -> Option<SchemaId> {
        match id {
            "schema_v1" => Some(SchemaId::Schema),
            "schema_field_v1" => Some(SchemaId::SchemaField),
            "key_group_v1" => Some(SchemaId::KeyGroup),
            "key_group_request_v1" => Some(SchemaId::KeyGroupRequest),
            "key_group_response_v1" => Some(SchemaId::KeyGroupResponse),
            _ => None,
        }
    }

    /// Returns a `SchemaId` given an application schema's name and view id.
    pub fn new_application(name: &str, view_id: &DocumentViewId) -> Self {
        SchemaId::Application(name.to_string(), view_id.clone())
    }

    /// Read an application schema id from a string.
    ///
    /// Parses the schema id by iteratively splitting sections from the right at `_` until the
    /// remainder is shorter than an operation id. Each section is parsed as an operation id
    /// and the last (leftmost) section is parsed as the schema's name.
    fn parse_application_schema_str(id_str: &str) -> Result<Self, SchemaIdError> {
        if id_str.find('_').is_none() {
            return Err(SchemaIdError::MalformedApplicationSchemaId(
                "expecting name and view id hashes separated by underscore".to_string(),
            ));
        }

        let mut operation_ids = vec![];
        let mut remainder = id_str;
        while let Some((left, right)) = remainder.rsplit_once('_') {
            // While we're parsing application schema ids here, let's not forget the possibility
            // that a system schema id is being thrown at this method. If that were to happen, the
            // version identifier part (e.g. `v1`) would end up in `right` in this loop iteration.
            // The following check let's us return a more helpful error in that situation than we
            // we would've gotten if we'd try and parse that as an `OperationId` right below.
            if right.starts_with('v') && right.len() < MAX_YAMF_HASH_SIZE * 2 {
                return Err(SchemaIdError::UnknownSystemSchema(id_str.to_string()));
            }

            operation_ids.push(right.parse::<OperationId>()?);

            // If the remainder is no longer than an entry hash we assume that it's the schema name.
            // By breaking here we allow the schema name to contain underscores as well.
            remainder = left;
            if remainder.len() < MAX_YAMF_HASH_SIZE * 2 {
                break;
            }
        }

        if remainder.is_empty() {
            return Err(SchemaIdError::MissingApplicationSchemaName(
                id_str.to_string(),
            ));
        }

        Ok(SchemaId::Application(
            remainder.to_string(),
            DocumentViewId::new(&operation_ids),
        ))
    }

    /// Returns schema id as string slice.
    pub fn as_str(&self) -> String {
        match self {
            SchemaId::Schema => "schema_v1".to_string(),
            SchemaId::SchemaField => "schema_field_v1".to_string(),
            SchemaId::KeyGroup => "key_group_v1".to_string(),
            SchemaId::KeyGroupResponse => "key_group_response_v1".to_string(),
            SchemaId::KeyGroupRequest => "key_group_request_v1".to_string(),
            SchemaId::Application(name, view_id) => {
                let mut schema_id = name.clone();
                for op_id in view_id.sorted().into_iter() {
                    schema_id.push('_');
                    schema_id.push_str(op_id.as_hash().as_str());
                }
                schema_id
            }
        }
    }
}

impl FromStr for SchemaId {
    type Err = SchemaIdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s)
    }
}

/// Serde `Visitor` implementation used to deserialize `SchemaId`.
struct SchemaIdVisitor;

impl<'de> Visitor<'de> for SchemaIdVisitor {
    type Value = SchemaId;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("schema id as string")
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        SchemaId::new(value).map_err(|err| serde::de::Error::custom(err.to_string()))
    }
}

impl Serialize for SchemaId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.as_str())
    }
}

impl<'de> Deserialize<'de> for SchemaId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_any(SchemaIdVisitor)
    }
}

#[cfg(test)]
mod test {
    use rstest::rstest;

    use crate::document::DocumentViewId;
    use crate::operation::OperationId;
    use crate::test_utils::constants::TEST_SCHEMA_ID;
    use crate::test_utils::fixtures::random_operation_id;

    use super::SchemaId;

    #[test]
    fn serialize() {
        let app_schema = SchemaId::new(TEST_SCHEMA_ID).unwrap();
        assert_eq!(
            serde_json::to_string(&app_schema).unwrap(),
            "\"venue_0020c65567ae37efea293e34a9c7d13f8f2bf23dbdc3b5c7b9ab46293111c48fc78b\""
        );

        let schema = SchemaId::Schema;
        assert_eq!(serde_json::to_string(&schema).unwrap(), "\"schema_v1\"");

        let schema_field = SchemaId::SchemaField;
        assert_eq!(
            serde_json::to_string(&schema_field).unwrap(),
            "\"schema_field_v1\""
        );
    }

    #[rstest]
    fn deserialize(
        #[from(random_operation_id)] op_id_1: OperationId,
        #[from(random_operation_id)] op_id_2: OperationId,
    ) {
        let app_schema = SchemaId::new_application(
            "venue",
            &DocumentViewId::new(&[op_id_1.clone(), op_id_2.clone()]),
        );
        assert_eq!(
            serde_json::from_str::<SchemaId>(&format!(
                "\"venue_{}_{}\"",
                op_id_1.as_hash().as_str(),
                op_id_2.as_hash().as_str()
            ))
            .unwrap(),
            app_schema
        );
        let schema = SchemaId::Schema;
        assert_eq!(
            serde_json::from_str::<SchemaId>("\"schema_v1\"").unwrap(),
            schema
        );
        let schema_field = SchemaId::SchemaField;
        assert_eq!(
            serde_json::from_str::<SchemaId>("\"schema_field_v1\"").unwrap(),
            schema_field
        );
    }

    // Not a hash at all
    #[rstest]
    #[case(
        "\"This is not a hash\"",
        "malformed application schema id: expecting name and view id hashes separated by \
        underscore at line 1 column 20"
    )]
    // An integer
    #[case(
        "5",
        "invalid type: integer `5`, expected schema id as string at line 1 column 1"
    )]
    // Only an operation id, could be interpreted as document view id but still missing the name
    #[case(
        "\"0020c65567ae37efea293e34a9c7d13f8f2bf23dbdc3b5c7b9ab46293111c48fc78b\"",
        "malformed application schema id: expecting name and view id hashes separated by \
        underscore at line 1 column 70"
    )]
    // Only the name is missing now
    #[case(
        "\"_0020c65567ae37efea293e34a9c7d13f8f2bf23dbdc3b5c7b9ab46293111c48fc78b\"",
        "application schema id is missing a name: _0020c65567ae37efea293e34a9c7d13f8f2bf23dbdc3b5c\
        7b9ab46293111c48fc78b at line 1 column 71"
    )]
    // This name is too long, parser will fail trying to read its last section as an operation id
    #[case(
        "\"this_name_is_way_too_long_it_cant_be_good_to_have_such_a_long_name_to_be_honest_0020c65\
        567ae37efea293e34a9c7d13f8f2bf23dbdc3b5c7b9ab46293111c48fc78b\"",
        "encountered invalid hash while parsing application schema id: invalid hex encoding in \
        hash string at line 1 column 150"
    )]
    // This hash is malformed
    #[case(
        "\"venue_0020c65567ae37efea293e34a9c7d13f8f2bf23dbdc3b5c7b9ab46293111c48fc7\"",
        "encountered invalid hash while parsing application schema id: invalid hash length 33 \
    bytes, expected 34 bytes at line 1 column 74"
    )]
    // this looks like a system schema, but it is not
    #[case(
        "\"unknown_system_schema_name_v1\"",
        "not a known system schema: unknown_system_schema_name_v1 at line 1 column 31"
    )]
    fn invalid_deserialization(#[case] schema_id: &str, #[case] expected_err: &str) {
        assert_eq!(
            format!(
                "{}",
                serde_json::from_str::<SchemaId>(schema_id).unwrap_err()
            ),
            expected_err
        );
    }

    #[test]
    fn new_schema_type() {
        let appl_schema = SchemaId::new(
            "venue_0020c65567ae37efea293e34a9c7d13f8f2bf23dbdc3b5c7b9ab46293111c48fc78b",
        )
        .unwrap();
        assert_eq!(
            appl_schema,
            SchemaId::new(
                "venue_0020c65567ae37efea293e34a9c7d13f8f2bf23dbdc3b5c7b9ab46293111c48fc78b"
            )
            .unwrap()
        );

        let schema = SchemaId::new("schema_v1").unwrap();
        assert_eq!(schema, SchemaId::Schema);

        let schema_field = SchemaId::new("schema_field_v1").unwrap();
        assert_eq!(schema_field, SchemaId::SchemaField);
    }

    #[test]
    fn parse_schema_type() {
        let schema: SchemaId = "schema_v1".parse().unwrap();
        assert_eq!(schema, SchemaId::Schema);
    }
}
