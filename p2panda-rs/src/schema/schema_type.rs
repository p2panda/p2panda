// SPDX-License-Identifier: AGPL-3.0-or-later
use serde::{de::Error, Deserialize, Deserializer, Serialize, Serializer};

use crate::hash::Hash;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SchemaType {
    Application(Hash),
    Schema,
    SchemaField,
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
