// SPDX-License-Identifier: AGPL-3.0-or-later

use std::fmt;
use std::str::FromStr;

use serde::de::{SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::document::DocumentViewId;
use crate::operation::{OperationId, PinnedRelation};
use crate::schema::error::SchemaIdError;

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
            hash_str => Ok(hash_str.parse::<DocumentViewId>()?.into()),
        }
    }
}

impl From<OperationId> for SchemaId {
    fn from(operation_id: OperationId) -> Self {
        Self::Application(PinnedRelation::new(operation_id.into()))
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
        formatter.write_str("string or sequence of operation id strings")
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
        let mut op_ids: Vec<OperationId> = Vec::new();

        while let Some(seq_value) = seq.next_element::<String>()? {
            match seq_value.parse::<OperationId>() {
                Ok(operation_id) => op_ids.push(operation_id),
                Err(hash_err) => {
                    return Err(serde::de::Error::custom(format!(
                        "Error parsing application schema id: {}",
                        hash_err
                    )))
                }
            };
        }

        let document_view_id = DocumentViewId::new(op_ids);
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
    use crate::operation::{OperationId, PinnedRelation};
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
    fn invalid_deserialization() {
        assert!(serde_json::from_str::<SchemaId>("[\"This is not a hash\"]").is_err());
        assert!(serde_json::from_str::<SchemaId>("5").is_err());
        assert!(serde_json::from_str::<SchemaId>(
            "0020c65567ae37efea293e34a9c7d13f8f2bf23dbdc3b5c7b9ab46293111c48fc78b"
        )
        .is_err());

        // Test invalid hash
        let invalid_hash = serde_json::from_str::<SchemaId>(
            "[\"0020c65567ae37efea293e34a9c7d13f8f2bf23dbdc3b5c7b9ab46293111c48fc7\"]",
        );
        assert_eq!(
            format!("{:?}", invalid_hash.unwrap_err()),
            "Error(\"Error parsing application schema id: invalid hash \
            length 33 bytes, expected 34 bytes\", line: 1, column: 70)"
        );

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
        let operation_id: OperationId =
            "00207b3a7de3470bfe34d34ea45472082c307b995b6bd4abe2ac4ee36edef5dea1b3"
                .parse()
                .unwrap();
        let schema: SchemaId = operation_id.clone().into();
        let document_view_id = DocumentViewId::new(vec![operation_id]);

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
