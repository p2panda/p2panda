// SPDX-License-Identifier: AGPL-3.0-or-later

use std::fmt::{self, Display};
use std::str::FromStr;

use serde::de::{SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::document::DocumentViewId;
use crate::hash::Hash;
use crate::operation::PinnedRelation;
use crate::schema::error::SchemaIdError;
use crate::Validate;

/// Identifies the schema of an [`crate::operation::Operation`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SchemaId {
    /// An application schema.
    Application(PinnedRelation),

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
            hash_str => Ok(SchemaId::from(Hash::new(hash_str)?)),
        }
    }
}

impl Display for SchemaId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let repr = match &self {
            SchemaId::Application(relation) => relation.view_id().hash().as_str().into(),
            SchemaId::Schema => "schema_v1".to_string(),
            SchemaId::SchemaField => "schema_field_v1".to_string(),
        };
        write!(f, "{}", repr)
    }
}

impl From<Hash> for SchemaId {
    fn from(hash: Hash) -> Self {
        Self::Application(PinnedRelation::new(DocumentViewId::new(vec![hash])))
    }
}

impl From<DocumentViewId> for SchemaId {
    fn from(view_id: DocumentViewId) -> Self {
        Self::Application(PinnedRelation::new(view_id))
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
        formatter.write_str("string or sequence of hash strings")
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        match value {
            "schema_v1" => Ok(SchemaId::Schema),
            "schema_field_v1" => Ok(SchemaId::SchemaField),
            _ => Err(serde::de::Error::custom(format!(
                "Unknown system schema name: {}",
                value
            ))),
        }
    }

    fn visit_seq<S>(self, mut seq: S) -> Result<Self::Value, S::Error>
    where
        S: SeqAccess<'de>,
    {
        let mut hashes: Vec<Hash> = Vec::new();

        while let Some(hash) = seq.next_element::<Hash>()? {
            if hash.validate().is_err() {
                return Err(serde::de::Error::custom(format!(
                    "Invalid hash {:?}",
                    hash.as_str()
                )));
            }

            hashes.push(hash);
        }

        let document_view_id = DocumentViewId::new(hashes);
        Ok(SchemaId::Application(PinnedRelation::new(document_view_id)))
    }
}

impl Serialize for SchemaId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match &self {
            SchemaId::Application(relation) => relation.serialize(serializer),
            SchemaId::Schema => serializer.serialize_str("schema_v1"),
            SchemaId::SchemaField => serializer.serialize_str("schema_field_v1"),
        }
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
    use crate::hash::Hash;
    use crate::operation::PinnedRelation;
    use crate::test_utils::constants::DEFAULT_SCHEMA_HASH;

    use super::SchemaId;

    #[test]
    fn serialize() {
        let app_schema = SchemaId::new(DEFAULT_SCHEMA_HASH).unwrap();
        assert_eq!(
            serde_json::to_string(&app_schema).unwrap(),
            "[\"0020c65567ae37efea293e34a9c7d13f8f2bf23dbdc3b5c7b9ab46293111c48fc78b\"]"
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
                "[\"0020c65567ae37efea293e34a9c7d13f8f2bf23dbdc3b5c7b9ab46293111c48fc78b\"]"
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
    fn string_representation() {
        let app_schema = SchemaId::new(DEFAULT_SCHEMA_HASH).unwrap();
        assert_eq!(
            format!("{}", app_schema),
            "0020505ecc036ed0fbac12acbc5cabe0efb985e53a7e36a71fc67fe0f50f631cd3ec"
        );
        assert_eq!(format!("{}", SchemaId::Schema), "schema_v1");
        assert_eq!(format!("{}", SchemaId::SchemaField), "schema_field_v1");
    }

    #[test]
    fn invalid_deserialization() {
        assert!(serde_json::from_str::<SchemaId>("[\"This is not a hash\"]").is_err());
        assert!(serde_json::from_str::<SchemaId>(
            "0020c65567ae37efea293e34a9c7d13f8f2bf23dbdc3b5c7b9ab46293111c48fc78b"
        )
        .is_err());
        assert!(serde_json::from_str::<SchemaId>("unknown_system_schema_name_v1").is_err());
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

    #[test]
    fn conversion() {
        let hash =
            Hash::new("00207b3a7de3470bfe34d34ea45472082c307b995b6bd4abe2ac4ee36edef5dea1b3")
                .unwrap();
        let schema: SchemaId = hash.clone().into();
        let document_view_id = DocumentViewId::new(vec![hash]);

        // From Hash
        assert_eq!(
            schema,
            SchemaId::Application(PinnedRelation::new(document_view_id.clone()))
        );

        // From DocumentViewId
        let schema: SchemaId = document_view_id.clone().into();
        assert_eq!(
            schema,
            SchemaId::Application(PinnedRelation::new(document_view_id))
        );
    }
}
