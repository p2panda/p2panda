// SPDX-License-Identifier: AGPL-3.0-or-later

use std::str::FromStr;

use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::document::DocumentId;
use crate::hash::Hash;
use crate::operation::Relation;
use crate::schema::error::SchemaIdError;

/// Identifies the schema of an [`crate::operation::Operation`].
#[derive(Clone, Debug, PartialEq)]
pub enum SchemaId {
    /// An application schema.
    Application(Relation),

    /// A schema definition.
    Schema,

    /// A schema definition field.
    SchemaField,
}

impl SchemaId {
    /// Instantiate a new `SchemaId` from a hash string.
    pub fn new(hash: &str) -> Result<Self, SchemaIdError> {
        match hash {
            "schema_v1" => Ok(SchemaId::Schema),
            "schema_field_v1" => Ok(SchemaId::SchemaField),
            string => {
                // We only use document_id in a relation at the moment.
                Ok(SchemaId::Application(Relation::new(DocumentId::new(
                    Hash::new(string)?,
                ))))
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

impl Serialize for SchemaId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(match &*self {
            SchemaId::Application(relation) => relation.document_id().as_str(),
            SchemaId::Schema => "schema_v1",
            SchemaId::SchemaField => "schema_field_v1",
        })
    }
}

impl<'de> Deserialize<'de> for SchemaId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;

        match s.as_str() {
            "schema_v1" => Ok(SchemaId::Schema),
            "schema_field_v1" => Ok(SchemaId::SchemaField),
            _ => match Hash::new(s.as_str()) {
                Ok(hash) => Ok(SchemaId::Application(Relation::new(DocumentId::new(hash)))),
                Err(e) => Err(SchemaIdError::HashError(e)).map_err(Error::custom),
            },
        }
    }
}

#[cfg(test)]
mod test {
    use crate::test_utils::constants::DEFAULT_SCHEMA_HASH;

    use super::SchemaId;

    #[test]
    fn serialize() {
        let app_schema = SchemaId::new(DEFAULT_SCHEMA_HASH).unwrap();
        assert_eq!(
            serde_json::to_string(&app_schema).unwrap(),
            "\"0020c65567ae37efea293e34a9c7d13f8f2bf23dbdc3b5c7b9ab46293111c48fc78b\""
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
        let app_schema = SchemaId::new(DEFAULT_SCHEMA_HASH).unwrap();
        assert_eq!(
            serde_json::from_str::<SchemaId>(
                "\"0020c65567ae37efea293e34a9c7d13f8f2bf23dbdc3b5c7b9ab46293111c48fc78b\""
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
    fn new_schema_type() {
        let appl_schema =
            SchemaId::new("0020c65567ae37efea293e34a9c7d13f8f2bf23dbdc3b5c7b9ab46293111c48fc78b")
                .unwrap();
        assert_eq!(
            appl_schema,
            SchemaId::new("0020c65567ae37efea293e34a9c7d13f8f2bf23dbdc3b5c7b9ab46293111c48fc78b")
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
