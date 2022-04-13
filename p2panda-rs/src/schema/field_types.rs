// SPDX-License-Identifier: AGPL-3.0-or-later

use std::str::FromStr;

use lazy_static::lazy_static;
use regex::Regex;

use crate::operation::OperationValue;

use super::{FieldTypeError, SchemaId};

/// Valid field types for publishing an application schema.
///
/// Implements conversion to `OperationValue`:
///
/// ```
/// # use p2panda_rs::operation::{OperationFields, OperationValue};
/// # use p2panda_rs::schema::FieldType;
/// let mut field_definition = OperationFields::new();
/// field_definition.add("name", OperationValue::Text("document_title".to_string()));
/// field_definition.add("type", FieldType::String.into());
/// ```
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

    /// Defines a [`Relation`][`crate::operation::Relation`] field that references the given
    /// schema.
    Relation(SchemaId),

    /// Defines a [`RelationList`][`crate::operation::RelationList`] field that references the
    /// given schema.
    RelationList(SchemaId),

    /// Defines a [`PinnedRelation`][`crate::operation::PinnedRelation`] field that references the
    /// given schema.
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

impl From<FieldType> for OperationValue {
    fn from(field_type: FieldType) -> OperationValue {
        OperationValue::Text(field_type.serialise())
    }
}

impl FromStr for FieldType {
    type Err = FieldTypeError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // Match non-parametric field types on their plain text name
        let text_match = match s {
            "bool" => Ok(FieldType::Bool),
            "int" => Ok(FieldType::Int),
            "float" => Ok(FieldType::Float),
            "str" => Ok(FieldType::String),
            _ => Err(FieldTypeError::InvalidFieldType(s.into())),
        };

        if text_match.is_ok() {
            return text_match;
        }

        // Matches a field type name, followed by an optional group in parentheses that contains
        // the referenced schema for relation field types
        lazy_static! {
            static ref RE: Regex = Regex::new(r"(\w+)(\((.+)\))?").unwrap();
        }
        let groups = RE.captures(s).unwrap();
        let name = groups.get(1).map(|m| m.as_str());
        let parameter = groups.get(3).map(|m| m.as_str());

        match (name, parameter) {
            (Some("relation"), Some(schema_id)) => {
                Ok(FieldType::Relation(SchemaId::new(schema_id)?))
            }
            (Some("relation_list"), Some(schema_id)) => {
                Ok(FieldType::RelationList(SchemaId::new(schema_id)?))
            }
            (Some("pinned_relation"), Some(schema_id)) => {
                Ok(FieldType::PinnedRelation(SchemaId::new(schema_id)?))
            }
            (Some("pinned_relation_list"), Some(schema_id)) => {
                Ok(FieldType::PinnedRelationList(SchemaId::new(schema_id)?))
            }
            _ => Err(FieldTypeError::InvalidFieldType(s.into())),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::operation::OperationValue;
    use crate::schema::{FieldType, SchemaId};
    use crate::Validate;

    #[test]
    fn serialises() {
        assert_eq!(FieldType::Bool.serialise(), "bool");
        assert!(OperationValue::from(FieldType::Bool).validate().is_ok());
        assert_eq!(FieldType::Int.serialise(), "int");
        assert_eq!(FieldType::Float.serialise(), "float");
        assert_eq!(FieldType::String.serialise(), "str");
        assert_eq!(
            FieldType::Relation(SchemaId::SchemaFieldDefinition(1)).serialise(),
            "relation(schema_field_definition_v1)"
        );
        assert_eq!(
            FieldType::RelationList(SchemaId::SchemaFieldDefinition(1)).serialise(),
            "relation_list(schema_field_definition_v1)"
        );
        assert_eq!(
            FieldType::PinnedRelation(SchemaId::SchemaFieldDefinition(1)).serialise(),
            "pinned_relation(schema_field_definition_v1)"
        );
        assert_eq!(
            FieldType::PinnedRelationList(SchemaId::SchemaFieldDefinition(1)).serialise(),
            "pinned_relation_list(schema_field_definition_v1)"
        );
    }

    #[test]
    fn deserialises() {
        assert_eq!(FieldType::Bool, "bool".parse().unwrap());
        assert_eq!(FieldType::Int, "int".parse().unwrap());
        assert_eq!(FieldType::Float, "float".parse().unwrap());
        assert_eq!(FieldType::String, "str".parse().unwrap());
        assert_eq!(
            FieldType::Relation(SchemaId::SchemaFieldDefinition(1)),
            "relation(schema_field_definition_v1)".parse().unwrap()
        );
        assert_eq!(
            FieldType::RelationList(SchemaId::SchemaFieldDefinition(1)),
            "relation_list(schema_field_definition_v1)".parse().unwrap()
        );
        assert_eq!(
            FieldType::PinnedRelation(SchemaId::SchemaFieldDefinition(1)),
            "pinned_relation(schema_field_definition_v1)"
                .parse()
                .unwrap()
        );
        assert_eq!(
            FieldType::PinnedRelationList(SchemaId::SchemaFieldDefinition(1)),
            "pinned_relation_list(schema_field_definition_v1)"
                .parse()
                .unwrap()
        );

        let invalid = "relation(no_no_no)".parse::<FieldType>();
        assert_eq!(
            invalid.unwrap_err().to_string(),
            "encountered invalid hash while parsing application schema id: invalid hex encoding \
            in hash string"
        );
    }

    #[test]
    fn invalid_type_string() {
        assert!("poopy".parse::<FieldType>().is_err());
    }
}
