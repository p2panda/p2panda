// SPDX-License-Identifier: AGPL-3.0-or-later

use std::fmt;
use std::str::FromStr;

use serde::de::Visitor;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use yasmf_hash::MAX_YAMF_HASH_SIZE;

use crate::document::DocumentViewId;
use crate::operation::OperationId;
use crate::schema::error::SchemaIdError;

/// Spelling of _schema definition_ schema
pub(super) const SCHEMA_DEFINITION_NAME: &str = "schema_definition";

/// Spelling of _schema field definition_ schema
pub(super) const SCHEMA_FIELD_DEFINITION_NAME: &str = "schema_field_definition";

/// Represent a schema's version.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SchemaVersion {
    /// An application schema's version contains its document view id.
    Application(DocumentViewId),

    /// A system schema's version contains an integer version number.
    System(u8),
}

/// Identifies the schema of an [`Operation`][`crate::operation::Operation`] or
/// [`Document`][`crate::document::Document`].
///
/// Every schema id has a name and version.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SchemaId {
    /// An application schema.
    Application(String, DocumentViewId),

    /// A schema definition.
    SchemaDefinition(u8),

    /// A schema definition field.
    SchemaFieldDefinition(u8),
}

impl SchemaId {
    /// Instantiate a new `SchemaId`.
    ///
    /// ```
    /// # use p2panda_rs::schema::SchemaId;
    /// let system_schema = SchemaId::new("schema_definition_v1");
    /// assert!(system_schema.is_ok());
    ///
    /// let application_schema = SchemaId::new(
    ///     "venue_0020c65567ae37efea293e34a9c7d13f8f2bf23dbdc3b5c7b9ab46293111c48fc78b"
    /// );
    /// assert!(application_schema.is_ok());
    /// ```
    pub fn new(id: &str) -> Result<Self, SchemaIdError> {
        // Retrieve the rightmost section separated by an underscore and check whether it follows
        // the version format of system schemas (e.g. `..._v1`).
        let rightmost_section = id
            .rsplit_once('_')
            .ok_or_else(|| {
                SchemaIdError::MalformedSchemaId("doesn't contain an underscore".to_string())
            })?
            .1;
        let is_system_schema =
            rightmost_section.starts_with('v') && rightmost_section.len() < MAX_YAMF_HASH_SIZE * 2;

        match is_system_schema {
            true => Self::parse_system_schema_str(id),
            false => Self::parse_application_schema_str(id),
        }
    }

    /// Returns a `SchemaId` given an application schema's name and view id.
    pub fn new_application(name: &str, view_id: &DocumentViewId) -> Self {
        Self::Application(name.to_string(), view_id.clone())
    }

    fn parse_system_schema_str(id_str: &str) -> Result<Self, SchemaIdError> {
        let (name, version_str) = id_str.rsplit_once('_').unwrap();
        let version = version_str[1..].parse::<u8>().map_err(|_| {
            SchemaIdError::MalformedSchemaId(format!(
                "couldn't parse system schema version from '{}'",
                id_str
            ))
        })?;
        match name {
            SCHEMA_DEFINITION_NAME => Ok(Self::SchemaDefinition(version)),
            SCHEMA_FIELD_DEFINITION_NAME => Ok(Self::SchemaFieldDefinition(version)),
            _ => Err(SchemaIdError::UnknownSystemSchema(name.to_string())),
        }
    }

    /// Read an application schema id from a string.
    ///
    /// Parses the schema id by iteratively splitting sections from the right at `_` until the
    /// remainder is shorter than an operation id. Each section is parsed as an operation id
    /// and the last (leftmost) section is parsed as the schema's name.
    fn parse_application_schema_str(id_str: &str) -> Result<Self, SchemaIdError> {
        let mut operation_ids = vec![];
        let mut remainder = id_str;
        while let Some((left, right)) = remainder.rsplit_once('_') {
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
            SchemaId::Application(name, view_id) => {
                let mut schema_id = name.to_string();
                for op_id in view_id.sorted().into_iter() {
                    schema_id.push('_');
                    schema_id.push_str(op_id.as_hash().as_str());
                }
                schema_id
            }
            SchemaId::SchemaDefinition(version) => {
                format!("{}_v{}", SCHEMA_DEFINITION_NAME, version)
            }
            SchemaId::SchemaFieldDefinition(version) => {
                format!("{}_v{}", SCHEMA_FIELD_DEFINITION_NAME, version)
            }
        }
    }

    /// Access the schema name.
    pub fn name(&self) -> &str {
        match self {
            SchemaId::Application(name, _) => name,
            SchemaId::SchemaDefinition(_) => SCHEMA_DEFINITION_NAME,
            SchemaId::SchemaFieldDefinition(_) => SCHEMA_FIELD_DEFINITION_NAME,
        }
    }

    /// Access the schema version.
    pub fn version(&self) -> SchemaVersion {
        match self {
            SchemaId::Application(_, view_id) => SchemaVersion::Application(view_id.clone()),
            SchemaId::SchemaDefinition(version) => SchemaVersion::System(*version),
            SchemaId::SchemaFieldDefinition(version) => SchemaVersion::System(*version),
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

        let schema = SchemaId::SchemaDefinition(1);
        assert_eq!(
            serde_json::to_string(&schema).unwrap(),
            "\"schema_definition_v1\""
        );

        let schema_field = SchemaId::SchemaFieldDefinition(1);
        assert_eq!(
            serde_json::to_string(&schema_field).unwrap(),
            "\"schema_field_definition_v1\""
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
        let schema = SchemaId::SchemaDefinition(1);
        assert_eq!(
            serde_json::from_str::<SchemaId>("\"schema_definition_v1\"").unwrap(),
            schema
        );
        let schema_field = SchemaId::SchemaFieldDefinition(1);
        assert_eq!(
            serde_json::from_str::<SchemaId>("\"schema_field_definition_v1\"").unwrap(),
            schema_field
        );
    }

    // Not a hash at all
    #[rstest]
    #[case(
        "\"This is not a hash\"",
        "malformed schema id: doesn't contain an underscore at line 1 column 20"
    )]
    // An integer
    #[case(
        "5",
        "invalid type: integer `5`, expected schema id as string at line 1 column 1"
    )]
    // Only an operation id, could be interpreted as document view id but still missing the name
    #[case(
        "\"0020c65567ae37efea293e34a9c7d13f8f2bf23dbdc3b5c7b9ab46293111c48fc78b\"",
        "malformed schema id: doesn't contain an underscore at line 1 column 70"
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
        "not a known system schema: unknown_system_schema_name at line 1 column 31"
    )]
    // malformed system schema version number
    #[case(
        "\"schema_definition_v1.5\"",
        "malformed schema id: couldn't parse system schema version from 'schema_definition_v1.5' \
        at line 1 column 24"
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

        let schema = SchemaId::new("schema_definition_v50").unwrap();
        assert_eq!(schema, SchemaId::SchemaDefinition(50));

        let schema_field = SchemaId::new("schema_field_definition_v1").unwrap();
        assert_eq!(schema_field, SchemaId::SchemaFieldDefinition(1));
    }

    #[test]
    fn parse_schema_type() {
        let schema: SchemaId = "schema_definition_v1".parse().unwrap();
        assert_eq!(schema, SchemaId::SchemaDefinition(1));
    }
}
