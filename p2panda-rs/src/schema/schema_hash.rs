// SPDX-License-Identifier: AGPL-3.0-or-later

use std::ops::Deref;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde::de::Error;

use crate::hash::Hash;
use crate::schema::error::SchemaHashError;

/// Enum representing existing schema types.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SchemaHash {
    /// An application schema with a hash.
    Application(Hash),
    
    /// A schema definition.
    Schema,
    
    /// A schema definition field.
    SchemaField,
}

impl SchemaHash {
    /// Instantiate a new SchemaHash from a hash string.
    pub fn new(hash: &str) -> Result<Self, SchemaHashError> {
        match hash {
            "00000000000000000000000000000000000000000000000000000000000000000001" => {
                Ok(SchemaHash::Schema)
            }
            "00000000000000000000000000000000000000000000000000000000000000000002" => {
                Ok(SchemaHash::SchemaField)
            }
            string => {
                let hash = Hash::new(string)?;
                Ok(SchemaHash::Application(hash))
            }
        }
    }
}

impl SchemaHash {
    fn as_str(&self) -> &str {
        match self {
            SchemaHash::Application(hash) => hash.as_str(),
            SchemaHash::Schema => {
                "00000000000000000000000000000000000000000000000000000000000000000001"
            }
            SchemaHash::SchemaField => {
                "00000000000000000000000000000000000000000000000000000000000000000002"
            }
        }
    }
}

impl FromStr for SchemaHash {
    type Err = SchemaHashError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::new(s)
    }
}

impl Deref for SchemaHash {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.as_str()
    }
}

impl Serialize for SchemaHash {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(match &*self {
            SchemaHash::Application(hash) => hash.as_str(),
            SchemaHash::Schema => {
                "00000000000000000000000000000000000000000000000000000000000000000001"
            }
            SchemaHash::SchemaField => {
                "00000000000000000000000000000000000000000000000000000000000000000002"
            }
        })
    }
}

impl<'de> Deserialize<'de> for SchemaHash {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;

        match s.as_str() {
            "00000000000000000000000000000000000000000000000000000000000000000001" => {
                Ok(SchemaHash::Schema)
            }
            "00000000000000000000000000000000000000000000000000000000000000000002" => {
                Ok(SchemaHash::SchemaField)
            }
            _ => match Hash::new(s.as_str()) {
                Ok(hash) => Ok(SchemaHash::Application(hash)),
                Err(e) => Err(SchemaHashError::HashError(e)).map_err(Error::custom),
            },
        }
    }
}

#[cfg(test)]
mod test {
    use crate::{hash::Hash, test_utils::constants::DEFAULT_SCHEMA_HASH};

    use super::SchemaHash;

    #[test]
    fn serialize() {
        let app_schema = SchemaHash::new(DEFAULT_SCHEMA_HASH).unwrap();
        assert_eq!(
            serde_json::to_string(&app_schema).unwrap(),
            "\"0020c65567ae37efea293e34a9c7d13f8f2bf23dbdc3b5c7b9ab46293111c48fc78b\""
        );
        let schema = SchemaHash::Schema;
        assert_eq!(
            serde_json::to_string(&schema).unwrap(),
            "\"00000000000000000000000000000000000000000000000000000000000000000001\""
        );
        let schema_field = SchemaHash::SchemaField;
        assert_eq!(
            serde_json::to_string(&schema_field).unwrap(),
            "\"00000000000000000000000000000000000000000000000000000000000000000002\""
        );
    }

    #[test]
    fn deserialize() {
        let app_schema = SchemaHash::new(DEFAULT_SCHEMA_HASH).unwrap();
        assert_eq!(
            serde_json::from_str::<SchemaHash>(
                "\"0020c65567ae37efea293e34a9c7d13f8f2bf23dbdc3b5c7b9ab46293111c48fc78b\""
            )
            .unwrap(),
            app_schema
        );
        let schema = SchemaHash::Schema;
        assert_eq!(
            serde_json::from_str::<SchemaHash>(
                "\"00000000000000000000000000000000000000000000000000000000000000000001\""
            )
            .unwrap(),
            schema
        );
        let schema_field = SchemaHash::SchemaField;
        assert_eq!(
            serde_json::from_str::<SchemaHash>(
                "\"00000000000000000000000000000000000000000000000000000000000000000002\""
            )
            .unwrap(),
            schema_field
        );
    }

    #[test]
    fn new_schema_type() {
        let appl_schema =
            SchemaHash::new("0020c65567ae37efea293e34a9c7d13f8f2bf23dbdc3b5c7b9ab46293111c48fc78b")
                .unwrap();
        assert_eq!(
            appl_schema,
            SchemaHash::Application(
                Hash::new("0020c65567ae37efea293e34a9c7d13f8f2bf23dbdc3b5c7b9ab46293111c48fc78b")
                    .unwrap()
            )
        );

        let schema =
            SchemaHash::new("00000000000000000000000000000000000000000000000000000000000000000001")
                .unwrap();
        assert_eq!(schema, SchemaHash::Schema);

        let schema_field =
            SchemaHash::new("00000000000000000000000000000000000000000000000000000000000000000002")
                .unwrap();
        assert_eq!(schema_field, SchemaHash::SchemaField);
    }

    #[test]
    fn parse_schema_type() {
        let schema: SchemaHash =
            "00000000000000000000000000000000000000000000000000000000000000000001"
                .parse()
                .unwrap();
        assert_eq!(schema, SchemaHash::Schema);
    }
}
