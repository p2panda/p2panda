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
    /// Returns the string representation of this type.
    pub fn to_string(&self) -> String {
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

impl From<FieldType> for String {
    fn from(field_type: FieldType) -> Self {
        field_type.to_string()
    }
}

#[cfg(test)]
mod tests {
    use crate::schema::{FieldType, SchemaId};

    #[test]
    fn serialises() {
        assert_eq!(FieldType::Bool.to_string(), "bool");
        assert_eq!(FieldType::Int.to_string(), "int");
        assert_eq!(FieldType::Float.to_string(), "float");
        assert_eq!(FieldType::String.to_string(), "str");
        assert_eq!(
            FieldType::Relation(SchemaId::SchemaField).to_string(),
            "relation(schema_field_v1)"
        );
        assert_eq!(
            FieldType::RelationList(SchemaId::SchemaField).to_string(),
            "relation_list(schema_field_v1)"
        );
        assert_eq!(
            FieldType::PinnedRelation(SchemaId::SchemaField).to_string(),
            "pinned_relation(schema_field_v1)"
        );
        assert_eq!(
            FieldType::PinnedRelationList(SchemaId::SchemaField).to_string(),
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
    fn into_string() {
        let bool_type: String = FieldType::Bool.into();
        assert_eq!(bool_type, "bool".to_string());
        let int_type: String = FieldType::Int.into();
        assert_eq!(int_type, "int".to_string());
        let type_float: String = FieldType::Float.into();
        assert_eq!(type_float, "float".to_string());
        let type_string: String = FieldType::String.into();
        assert_eq!(type_string, "str".to_string());
        let type_relation: String = FieldType::Relation(SchemaId::Schema).into();
        assert_eq!(type_relation, "relation(schema_v1)".to_string());
        let type_relation_list: String = FieldType::RelationList(SchemaId::Schema).into();
        assert_eq!(type_relation_list, "relation_list(schema_v1)".to_string());
        let type_pinned_relation: String = FieldType::PinnedRelation(SchemaId::Schema).into();
        assert_eq!(
            type_pinned_relation,
            "pinned_relation(schema_v1)".to_string()
        );
        let type_pinned_relation_list: String =
            FieldType::PinnedRelationList(SchemaId::Schema).into();
        assert_eq!(
            type_pinned_relation_list,
            "pinned_relation_list(schema_v1)".to_string()
        );
    }

    #[test]
    fn invalid_type_string() {
        assert!("poopy".parse::<FieldType>().is_err());
    }
}
