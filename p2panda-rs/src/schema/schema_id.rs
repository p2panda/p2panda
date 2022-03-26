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
}

impl SchemaId {
    /// Instantiate a new `SchemaId` from a hash string or system schema name.
    ///
    /// If a hash string is passed, it will be converted into a document view id with only one hash
    /// inside.
    pub fn new(id: &str) -> Result<Self, SchemaIdError> {
        match id {
            "schema_v1" => Ok(SchemaId::Schema),
            "schema_field_v1" => Ok(SchemaId::SchemaField),
            application_schema_id => Ok(Self::parse_application_schema_str(application_schema_id)?),
        }
    }

    /// Returns a `SchemaId` given an application schema's name and view id.
    pub fn new_application(name: &str, view_id: &DocumentViewId) -> Self {
        SchemaId::Application(name.to_string(), view_id.clone())
    }

    /// Parse an application schema id from a string
    fn parse_application_schema_str(id_str: &str) -> Result<Self, SchemaIdError> {
        let mut operation_ids = vec![];
        let mut remainder = id_str.to_string();

        // Iteratively split at `_` from the right
        while let Some((left, right)) = remainder.rsplit_once('_') {
            // Catch trying to parse an unknown system schema
            if right.starts_with('v') && right.len() < MAX_YAMF_HASH_SIZE * 2 {
                return Err(SchemaIdError::UnknownSystemSchema(id_str.to_string()));
            }

            operation_ids.push(right.parse::<OperationId>()?);

            // If the remainder is shorter than an entry hash we assume that it's the schema name.
            remainder = left.to_string();
            if remainder.chars().count() <= MAX_YAMF_HASH_SIZE * 2 {
                break;
            }
        }

        if remainder.is_empty() {
            return Err(SchemaIdError::InvalidApplicationSchemaId(
                "missing schema name".to_string(),
            ));
        }

        Ok(SchemaId::Application(
            remainder,
            DocumentViewId::new(&operation_ids),
        ))
    }

    fn as_str(&self) -> String {
        match self {
            SchemaId::Schema => "schema_v1".to_string(),
            SchemaId::SchemaField => "schema_field_v1".to_string(),
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
        formatter.write_str("string or sequence of operation id strings")
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
    use crate::document::DocumentViewId;
    use crate::test_utils::constants::DEFAULT_SCHEMA_HASH;

    use super::SchemaId;

    #[test]
    fn serialize() {
        let app_schema = SchemaId::new_application(
            "venue",
            &DEFAULT_SCHEMA_HASH.parse::<DocumentViewId>().unwrap(),
        );
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

    #[test]
    fn deserialize() {
        let app_schema = SchemaId::new_application(
            "venue",
            &DEFAULT_SCHEMA_HASH.parse::<DocumentViewId>().unwrap(),
        );
        assert_eq!(
            serde_json::from_str::<SchemaId>(
                "\"venue_0020c65567ae37efea293e34a9c7d13f8f2bf23dbdc3b5c7b9ab46293111c48fc78b\""
            )
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

    #[test]
    fn invalid_deserialization() {
        assert!(serde_json::from_str::<SchemaId>("[\"This is not a hash\"]").is_err());
        assert!(serde_json::from_str::<SchemaId>("5").is_err());
        assert!(serde_json::from_str::<SchemaId>(
            "0020c65567ae37efea293e34a9c7d13f8f2bf23dbdc3b5c7b9ab46293111c48fc78b"
        )
        .is_err());

        // Test invalid hash
        let invalid_hash = serde_json::from_str::<SchemaId>(
            "\"venue_0020c65567ae37efea293e34a9c7d13f8f2bf23dbdc3b5c7b9ab46293111c48fc7\"",
        );
        assert_eq!(
            format!("{}", invalid_hash.unwrap_err()),
            "encountered invalid hash while parsing application schema id: invalid hash length 33 \
            bytes, expected 34 bytes at line 1 column 74"
        );

        assert_eq!(
            "not a known system schema: unknown_system_schema_name_v1 at line 1 column 31",
            format!(
                "{}",
                serde_json::from_str::<SchemaId>("\"unknown_system_schema_name_v1\"").unwrap_err()
            )
        );

        // Test missing schema name
        let missing_name = serde_json::from_str::<SchemaId>(
            "\"_0020c65567ae37efea293e34a9c7d13f8f2bf23dbdc3b5c7b9ab46293111c48fc78b\"",
        );
        assert_eq!(
            format!("{}", missing_name.unwrap_err()),
            "invalid application schema id: missing schema name at line 1 column 71"
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
