// SPDX-License-Identifier: AGPL-3.0-or-later

use std::str::FromStr;

use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::document::DocumentId;
use crate::hash::Hash;
use crate::operation::Relation;
use crate::schema::error::SchemaIdError;

/// Struct representing a SchemaV1 id.
#[derive(Clone, Debug, PartialEq)]
pub struct SchemaV1(String);

impl SchemaV1 {
    fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl Default for SchemaV1 {
    fn default() -> Self {
        Self("schema_v1".to_string())
    }
}

/// Struct representing a SchemaFieldV1 id.
#[derive(Clone, Debug, PartialEq)]
pub struct SchemaFieldV1(String);

impl SchemaFieldV1 {
    fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl Default for SchemaFieldV1 {
    fn default() -> Self {
        Self("schema_field_v1".to_string())
    }
}

/// Identifies the schema of an [`crate::operation::Operation`].
#[derive(Clone, Debug, PartialEq)]
pub enum SchemaId {
    /// An application schema.
    Application(Relation),

    /// A schema definition.
    Schema(SchemaV1),

    /// A schema definition field.
    SchemaField(SchemaFieldV1),
}

impl SchemaId {
    /// Instantiate a new `SchemaId` from a hash string.
    pub fn new(hash: &str) -> Result<Self, SchemaIdError> {
        match hash {
            "schema_v1" => Ok(SchemaId::Schema(SchemaV1::default())),
            "schema_field_v1" => Ok(SchemaId::SchemaField(SchemaFieldV1::default())),
            string => {
                // We only use document_id in a relation at the moment.
                Ok(SchemaId::Application(Relation::new(DocumentId::new(
                    Hash::new(string)?,
                ))))
            }
        }
    }
}

impl From<Hash> for SchemaId {
    fn from(hash: Hash) -> Self {
        Self::Application(Relation::new(DocumentId::new(hash)))
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
            SchemaId::Schema(schema) => schema.as_str(),
            SchemaId::SchemaField(schema) => schema.as_str(),
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
            "schema_v1" => Ok(SchemaId::Schema(SchemaV1::default())),
            "schema_field_v1" => Ok(SchemaId::SchemaField(SchemaFieldV1::default())),
            _ => match Hash::new(s.as_str()) {
                Ok(hash) => Ok(SchemaId::Application(Relation::new(DocumentId::new(hash)))),
                Err(e) => Err(SchemaIdError::HashError(e)).map_err(Error::custom),
            },
        }
    }
}

#[cfg(test)]
mod test {
    use crate::document::DocumentId;
    use crate::hash::Hash;
    use crate::operation::Relation;
    use crate::schema::schema_id::{SchemaFieldV1, SchemaV1};
    use crate::test_utils::constants::DEFAULT_SCHEMA_HASH;

    use super::SchemaId;

    #[test]
    fn serialize() {
        let app_schema = SchemaId::new(DEFAULT_SCHEMA_HASH).unwrap();
        assert_eq!(
            serde_json::to_string(&app_schema).unwrap(),
            "\"0020c65567ae37efea293e34a9c7d13f8f2bf23dbdc3b5c7b9ab46293111c48fc78b\""
        );
        let schema = SchemaId::Schema(SchemaV1::default());
        assert_eq!(serde_json::to_string(&schema).unwrap(), "\"schema_v1\"");
        let schema_field = SchemaId::SchemaField(SchemaFieldV1::default());
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
        let schema = SchemaId::Schema(SchemaV1::default());
        assert_eq!(
            serde_json::from_str::<SchemaId>("\"schema_v1\"").unwrap(),
            schema
        );
        let schema_field = SchemaId::SchemaField(SchemaFieldV1::default());
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
        assert_eq!(schema, SchemaId::Schema(SchemaV1::default()));

        let schema_field = SchemaId::new("schema_field_v1").unwrap();
        assert_eq!(
            schema_field,
            SchemaId::SchemaField(SchemaFieldV1::default())
        );
    }

    #[test]
    fn parse_schema_type() {
        let schema: SchemaId = "schema_v1".parse().unwrap();
        assert_eq!(schema, SchemaId::Schema(SchemaV1::default()));
    }

    #[test]
    fn conversion() {
        let hash =
            Hash::new("00207b3a7de3470bfe34d34ea45472082c307b995b6bd4abe2ac4ee36edef5dea1b3")
                .unwrap();
        let schema: SchemaId = hash.clone().into();

        assert_eq!(
            schema,
            SchemaId::Application(Relation::new(DocumentId::new(hash)))
        );
    }
}
