// SPDX-License-Identifier: AGPL-3.0-or-later

use regex::Regex;
use std::str::FromStr;

use super::{FieldTypeError, SchemaId};

/// Valid field types for publishing an application schema.
#[derive(Clone, Debug, PartialEq)]
pub enum FieldType {
    /// Defines a boolean field.
    Bool,

    /// Defines an integer number field.
    Int,

    /// Defines a floating point number field.
    Float,

    /// Defines a text string field.
    String,

    /// Defines a [`Relation`][`crate::operation::Relation`] field that references the given schema.
    Relation(SchemaId),

    /// Defines a [`RelationList`][`crate::operation::RelationList`] field that references the
    /// given schema.
    RelationList(SchemaId),

    /// Defines a [`PinnedRelation`][`crate::operation::PinnedRelation`] field that references
    /// the given schema.
    PinnedRelation(SchemaId),

    /// Defines a [`PinnedRelationList`][`crate::operation::PinnedRelationList`] field that
    /// references the given schema.
    PinnedRelationList(SchemaId),
}

impl FieldType {
    /// Serialises this field type to text.
    pub fn serialise(&self) -> String {
        match self {
            FieldType::Bool => "bool".to_string(),
            FieldType::Int => "int".to_string(),
            FieldType::Float => "float".to_string(),
            FieldType::String => "str".to_string(),
            FieldType::Relation(schema_id) => format!("relation({})", schema_id.as_str()),
            FieldType::RelationList(schema_id) => format!("relation_list({})", schema_id.as_str()),
            FieldType::PinnedRelation(schema_id) => {
                format!("pinned_relation({})", schema_id.as_str())
            }
            FieldType::PinnedRelationList(schema_id) => {
                format!("pinned_relation_list({})", schema_id.as_str())
            }
        }
    }
}

impl FromStr for FieldType {
    type Err = FieldTypeError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // Matches a field type, followed an optional group in paranetheses that contains the
        // referenced schema for relation field types.
        let re = Regex::new(r"(\w+)(\((.+)\))?").unwrap();
        let groups = re.captures(s).unwrap();

        match (
            groups.get(1).map(|m| m.as_str()),
            groups.get(3).map(|m| m.as_str()),
        ) {
            (Some("bool"), None) => Ok(FieldType::Bool),
            (Some("int"), None) => Ok(FieldType::Int),
            (Some("float"), None) => Ok(FieldType::Float),
            (Some("str"), None) => Ok(FieldType::String),
            (Some("relation"), Some(schema_id)) => {
                Ok(FieldType::Relation(SchemaId::new(schema_id).unwrap()))
            }
            (Some("relation_list"), Some(schema_id)) => {
                Ok(FieldType::RelationList(SchemaId::new(schema_id).unwrap()))
            }
            (Some("pinned_relation"), Some(schema_id)) => {
                Ok(FieldType::PinnedRelation(SchemaId::new(schema_id).unwrap()))
            }
            (Some("pinned_relation_list"), Some(schema_id)) => Ok(FieldType::PinnedRelationList(
                SchemaId::new(schema_id).unwrap(),
            )),
            _ => Err(FieldTypeError::InvalidFieldType(s.into())),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::schema::{FieldType, SchemaId};

    #[test]
    fn serialises() {
        assert_eq!(FieldType::Bool.serialise(), "bool");
        assert_eq!(FieldType::Int.serialise(), "int");
        assert_eq!(FieldType::Float.serialise(), "float");
        assert_eq!(FieldType::String.serialise(), "str");
        assert_eq!(
            FieldType::Relation(SchemaId::SchemaField).serialise(),
            "relation(schema_field_v1)"
        );
        assert_eq!(
            FieldType::RelationList(SchemaId::SchemaField).serialise(),
            "relation_list(schema_field_v1)"
        );
        assert_eq!(
            FieldType::PinnedRelation(SchemaId::SchemaField).serialise(),
            "pinned_relation(schema_field_v1)"
        );
        assert_eq!(
            FieldType::PinnedRelationList(SchemaId::SchemaField).serialise(),
            "pinned_relation_list(schema_field_v1)"
        );
    }

    #[test]
    fn deserialises() {
        assert_eq!(FieldType::Bool, "bool".parse().unwrap());
        assert_eq!(FieldType::Int, "int".parse().unwrap());
        assert_eq!(FieldType::Float, "float".parse().unwrap());
        assert_eq!(FieldType::String, "str".parse().unwrap());
        assert_eq!(
            FieldType::Relation(SchemaId::SchemaField),
            "relation(schema_field_v1)".parse().unwrap()
        );
        assert_eq!(
            FieldType::RelationList(SchemaId::SchemaField),
            "relation_list(schema_field_v1)".parse().unwrap()
        );
        assert_eq!(
            FieldType::PinnedRelation(SchemaId::SchemaField),
            "pinned_relation(schema_field_v1)".parse().unwrap()
        );
        assert_eq!(
            FieldType::PinnedRelationList(SchemaId::SchemaField),
            "pinned_relation_list(schema_field_v1)".parse().unwrap()
        );
    }

    #[test]
    fn invalid_type_string() {
        assert!("poopy".parse::<FieldType>().is_err());
    }
}
