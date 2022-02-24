// SPDX-License-Identifier: AGPL-3.0-or-later

use std::ops::Deref;
use std::str::FromStr;

use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::hash::Hash;
use crate::schema::error::SchemaIdError;

/// Enum representing existing schema types.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SchemaId {
    /// An application schema with a hash.
    Application(Hash),

    /// A schema definition.
    Schema,

    /// A schema definition field.
    SchemaField,
}

impl SchemaId {
    /// Instantiate a new SchemaId from a hash string.
    pub fn new(hash: &str) -> Result<Self, SchemaIdError> {
        match hash {
            "SCHEMA_V1" => Ok(SchemaId::Schema),
            "SCHEMA_FIELD_V1" => Ok(SchemaId::SchemaField),
            string => {
                let hash = Hash::new(string)?;
                Ok(SchemaId::Application(hash))
            }
        }
    }
}

impl SchemaId {
    fn as_str(&self) -> &str {
        match self {
            SchemaId::Application(hash) => hash.as_str(),
            SchemaId::Schema => "SCHEMA_V1",
            SchemaId::SchemaField => "SCHEMA_FIELD_V1",
        }
    }
}

impl FromStr for SchemaId {
    type Err = SchemaIdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s)
    }
}

impl Deref for SchemaId {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}

impl Serialize for SchemaId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(match &*self {
            SchemaId::Application(hash) => hash.as_str(),
            SchemaId::Schema => "SCHEMA_V1",
            SchemaId::SchemaField => "SCHEMA_FIELD_V1",
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
            "SCHEMA_V1" => Ok(SchemaId::Schema),
            "SCHEMA_FIELD_V1" => Ok(SchemaId::SchemaField),
            _ => match Hash::new(s.as_str()) {
                Ok(hash) => Ok(SchemaId::Application(hash)),
                Err(e) => Err(SchemaIdError::HashError(e)).map_err(Error::custom),
            },
        }
    }
}

#[cfg(test)]
mod test {
    use crate::{hash::Hash, test_utils::constants::DEFAULT_SCHEMA_HASH};

    use super::SchemaId;

    #[test]
    fn serialize() {
        let app_schema = SchemaId::new(DEFAULT_SCHEMA_HASH).unwrap();
        assert_eq!(
            serde_json::to_string(&app_schema).unwrap(),
            "\"0020c65567ae37efea293e34a9c7d13f8f2bf23dbdc3b5c7b9ab46293111c48fc78b\""
        );
        let schema = SchemaId::Schema;
        assert_eq!(serde_json::to_string(&schema).unwrap(), "\"SCHEMA_V1\"");
        let schema_field = SchemaId::SchemaField;
        assert_eq!(
            serde_json::to_string(&schema_field).unwrap(),
            "\"SCHEMA_FIELD_V1\""
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
            serde_json::from_str::<SchemaId>("\"SCHEMA_V1\"").unwrap(),
            schema
        );
        let schema_field = SchemaId::SchemaField;
        assert_eq!(
            serde_json::from_str::<SchemaId>("\"SCHEMA_FIELD_V1\"").unwrap(),
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
            SchemaId::Application(
                Hash::new("0020c65567ae37efea293e34a9c7d13f8f2bf23dbdc3b5c7b9ab46293111c48fc78b")
                    .unwrap()
            )
        );

        let schema = SchemaId::new("SCHEMA_V1").unwrap();
        assert_eq!(schema, SchemaId::Schema);

        let schema_field = SchemaId::new("SCHEMA_FIELD_V1").unwrap();
        assert_eq!(schema_field, SchemaId::SchemaField);
    }

    #[test]
    fn parse_schema_type() {
        let schema: SchemaId = "SCHEMA_V1".parse().unwrap();
        assert_eq!(schema, SchemaId::Schema);
    }
}
