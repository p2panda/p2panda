// SPDX-License-Identifier: AGPL-3.0-or-later
use serde::{de::Error, Deserialize, Deserializer, Serialize, Serializer};

use crate::hash::{Hash, HashError};

/// Enum representing existing schema types
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SchemaType {
    /// An application schema with a hash
    Application(Hash),
    /// A schema definition
    Schema,
    /// A schema definition field
    SchemaField,
}

impl SchemaType {
    /// Instantiate a new SchemaType from a hash string.
    pub fn new(hash: &str) -> Result<Self, HashError> {
        match hash {
            "00000000000000000000000000000000000000000000000000000000000000000001" => {
                Ok(SchemaType::Schema)
            }
            "00000000000000000000000000000000000000000000000000000000000000000002" => {
                Ok(SchemaType::SchemaField)
            }
            string => {
                let hash = Hash::new(string)?;
                Ok(SchemaType::Application(hash))
            }
        }
    }
}

impl Serialize for SchemaType {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(match &*self {
            SchemaType::Application(hash) => hash.as_str(),
            SchemaType::Schema => {
                "00000000000000000000000000000000000000000000000000000000000000000001"
            }
            SchemaType::SchemaField => {
                "00000000000000000000000000000000000000000000000000000000000000000002"
            }
        })
    }
}

impl<'de> Deserialize<'de> for SchemaType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;

        match s.as_str() {
            "00000000000000000000000000000000000000000000000000000000000000000001" => {
                Ok(SchemaType::Schema)
            }
            "00000000000000000000000000000000000000000000000000000000000000000002" => {
                Ok(SchemaType::SchemaField)
            }
            _ => {
                let hash = Hash::new(s.as_str()).map_err(Error::custom)?;
                Ok(SchemaType::Application(hash))
            }
        }
    }
}

#[cfg(test)]
mod test {
    use crate::{hash::Hash, test_utils::constants::DEFAULT_SCHEMA_HASH};

    use super::SchemaType;

    #[test]
    fn serialize() {
        let app_schema = SchemaType::new(DEFAULT_SCHEMA_HASH).unwrap();
        assert_eq!(
            serde_json::to_string(&app_schema).unwrap(),
            "\"0020c65567ae37efea293e34a9c7d13f8f2bf23dbdc3b5c7b9ab46293111c48fc78b\""
        );
        let schema = SchemaType::Schema;
        assert_eq!(
            serde_json::to_string(&schema).unwrap(),
            "\"00000000000000000000000000000000000000000000000000000000000000000001\""
        );
        let schema_field = SchemaType::SchemaField;
        assert_eq!(
            serde_json::to_string(&schema_field).unwrap(),
            "\"00000000000000000000000000000000000000000000000000000000000000000002\""
        );
    }

    #[test]
    fn deserialize() {
        let app_schema = SchemaType::new(DEFAULT_SCHEMA_HASH).unwrap();
        assert_eq!(
            serde_json::from_str::<SchemaType>(
                "\"0020c65567ae37efea293e34a9c7d13f8f2bf23dbdc3b5c7b9ab46293111c48fc78b\""
            )
            .unwrap(),
            app_schema
        );
        let schema = SchemaType::Schema;
        assert_eq!(
            serde_json::from_str::<SchemaType>(
                "\"00000000000000000000000000000000000000000000000000000000000000000001\""
            )
            .unwrap(),
            schema
        );
        let schema_field = SchemaType::SchemaField;
        assert_eq!(
            serde_json::from_str::<SchemaType>(
                "\"00000000000000000000000000000000000000000000000000000000000000000002\""
            )
            .unwrap(),
            schema_field
        );
    }

    #[test]
    fn new_schema_type() {
        let appl_schema =
            SchemaType::new("0020c65567ae37efea293e34a9c7d13f8f2bf23dbdc3b5c7b9ab46293111c48fc78b")
                .unwrap();
        assert_eq!(
            appl_schema,
            SchemaType::Application(
                Hash::new("0020c65567ae37efea293e34a9c7d13f8f2bf23dbdc3b5c7b9ab46293111c48fc78b")
                    .unwrap()
            )
        );

        let schema =
            SchemaType::new("00000000000000000000000000000000000000000000000000000000000000000001")
                .unwrap();
        assert_eq!(schema, SchemaType::Schema);

        let schema_field =
            SchemaType::new("00000000000000000000000000000000000000000000000000000000000000000002")
                .unwrap();
        assert_eq!(schema_field, SchemaType::SchemaField);
    }
}
