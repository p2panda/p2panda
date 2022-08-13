// SPDX-License-Identifier: AGPL-3.0-or-later

use std::fmt::Display;
use std::str::FromStr;

use lazy_static::lazy_static;
use regex::Regex;

use crate::operation::OperationValue;
use crate::schema::error::FieldTypeError;
use crate::schema::SchemaId;

/// Valid field types for publishing an application schema.
///
/// Implements conversion to `OperationValue`:
///
/// ```
/// # use p2panda_rs::operation::{OperationFields, OperationValue};
/// # use p2panda_rs::schema::FieldType;
/// let mut field_definition = OperationFields::new();
/// field_definition.insert("name", "document_title".into());
/// field_definition.insert("type", FieldType::String.into());
/// ```
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum FieldType {
    /// Defines a boolean field.
    Boolean,

    /// Defines an integer number field.
    Integer,

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

/// Returns string representation of this field type.
impl Display for FieldType {
    // Note: This automatically implements the `to_string` function as well.
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let field_type_str = match self {
            FieldType::Boolean => "bool".to_string(),
            FieldType::Integer => "int".to_string(),
            FieldType::Float => "float".to_string(),
            FieldType::String => "str".to_string(),
            FieldType::Relation(schema_id) => format!("relation({})", schema_id),
            FieldType::RelationList(schema_id) => {
                format!("relation_list({})", schema_id)
            }
            FieldType::PinnedRelation(schema_id) => {
                format!("pinned_relation({})", schema_id)
            }
            FieldType::PinnedRelationList(schema_id) => {
                format!("pinned_relation_list({})", schema_id)
            }
        };

        write!(f, "{}", field_type_str)
    }
}

impl From<FieldType> for OperationValue {
    fn from(field_type: FieldType) -> OperationValue {
        OperationValue::String(field_type.to_string())
    }
}

impl FromStr for FieldType {
    type Err = FieldTypeError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // Match non-parametric field types on their plain text name
        let text_match = match s {
            "bool" => Ok(FieldType::Boolean),
            "int" => Ok(FieldType::Integer),
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
            // Unwrap as we checked the regular expression for correctness
            static ref RELATION_REGEX: Regex = Regex::new(r"(\w+)(\((.+)\))?").unwrap();
        }

        // @TODO: This might panic if input is invalid?
        let groups = RELATION_REGEX.captures(s).unwrap();
        let relation_type = groups.get(1).map(|group_match| group_match.as_str());
        let schema_id = groups.get(3).map(|group_match| group_match.as_str());

        match (relation_type, schema_id) {
            (Some("relation"), Some(schema_id)) => {
                Ok(FieldType::Relation(SchemaId::from_str(schema_id)?))
            }
            (Some("relation_list"), Some(schema_id)) => {
                Ok(FieldType::RelationList(SchemaId::from_str(schema_id)?))
            }
            (Some("pinned_relation"), Some(schema_id)) => {
                Ok(FieldType::PinnedRelation(SchemaId::from_str(schema_id)?))
            }
            (Some("pinned_relation_list"), Some(schema_id)) => Ok(FieldType::PinnedRelationList(
                SchemaId::from_str(schema_id)?,
            )),
            _ => Err(FieldTypeError::InvalidFieldType(s.into())),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::schema::SchemaId;

    use super::FieldType;

    #[test]
    fn to_string() {
        assert_eq!(FieldType::Boolean.to_string(), "bool");
        assert_eq!(FieldType::Integer.to_string(), "int");
        assert_eq!(FieldType::Float.to_string(), "float");
        assert_eq!(FieldType::String.to_string(), "str");
        assert_eq!(
            FieldType::Relation(SchemaId::SchemaFieldDefinition(1)).to_string(),
            "relation(schema_field_definition_v1)"
        );
        assert_eq!(
            FieldType::RelationList(SchemaId::SchemaFieldDefinition(1)).to_string(),
            "relation_list(schema_field_definition_v1)"
        );
        assert_eq!(
            FieldType::PinnedRelation(SchemaId::SchemaFieldDefinition(1)).to_string(),
            "pinned_relation(schema_field_definition_v1)"
        );
        assert_eq!(
            FieldType::PinnedRelationList(SchemaId::SchemaFieldDefinition(1)).to_string(),
            "pinned_relation_list(schema_field_definition_v1)"
        );
    }

    #[test]
    fn from_str() {
        assert_eq!(FieldType::Boolean, "bool".parse().unwrap());
        assert_eq!(FieldType::Integer, "int".parse().unwrap());
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
