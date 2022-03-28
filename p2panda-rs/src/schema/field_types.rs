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

    /// Defines a [`Relation`][`crate::operation::Relation`] field.
    Relation(SchemaId),

    /// Defines a [`RelationList`][`crate::operation::RelationList`] field.
    RelationList(SchemaId),

    /// Defines a [`PinnedRelation`][`crate::operation::PinnedRelation`] field.
    PinnedRelation(SchemaId),

    /// Defines a [`PinnedRelationList`][`crate::operation::PinnedRelationList`] field.
    PinnedRelationList(SchemaId),
}

impl FieldType {
    /// Returns the string representation of this type.
    pub fn as_str(&self) -> String {
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
        field_type.as_str()
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use crate::schema::{FieldType, SchemaId};
    use crate::test_utils::fixtures::schema;

    #[rstest]
    fn serialises(schema: SchemaId) {
        assert_eq!(FieldType::Bool.as_str(), "bool");
        assert_eq!(FieldType::Int.as_str(), "int");
        assert_eq!(FieldType::Float.as_str(), "float");
        assert_eq!(FieldType::String.as_str(), "str");
        assert_eq!(FieldType::Relation(schema.clone()).as_str(), "relation");
        assert_eq!(
            FieldType::RelationList(schema.clone()).as_str(),
            "relation_list"
        );
        assert_eq!(
            FieldType::PinnedRelation(schema.clone()).as_str(),
            "pinned_relation"
        );
        assert_eq!(
            FieldType::PinnedRelationList(schema).as_str(),
            "pinned_relation_list"
        );
    }
    #[rstest]
    fn deserialises(schema: SchemaId) {
        assert_eq!(FieldType::Bool, "bool".parse().unwrap());
        assert_eq!(FieldType::Int, "int".parse().unwrap());
        assert_eq!(FieldType::Float, "float".parse().unwrap());
        assert_eq!(FieldType::String, "str".parse().unwrap());
        assert_eq!(
            FieldType::Relation(schema.clone()),
            "relation".parse().unwrap()
        );
        assert_eq!(
            FieldType::RelationList(schema.clone()),
            "relation_list".parse().unwrap()
        );
        assert_eq!(
            FieldType::PinnedRelation(schema.clone()),
            "pinned_relation".parse().unwrap()
        );
        assert_eq!(
            FieldType::PinnedRelationList(schema),
            "pinned_relation_list".parse().unwrap()
        );
    }
    #[rstest]
    fn into_string(schema: SchemaId) {
        let bool_type: String = FieldType::Bool.into();
        assert_eq!(bool_type, "bool".to_string());
        let int_type: String = FieldType::Int.into();
        assert_eq!(int_type, "int".to_string());
        let type_float: String = FieldType::Float.into();
        assert_eq!(type_float, "float".to_string());
        let type_string: String = FieldType::String.into();
        assert_eq!(type_string, "str".to_string());
        let type_relation: String = FieldType::Relation(schema.clone()).into();
        assert_eq!(type_relation, "relation".to_string());
        let type_relation_list: String = FieldType::RelationList(schema.clone()).into();
        assert_eq!(type_relation_list, "relation_list".to_string());
        let type_pinned_relation: String = FieldType::PinnedRelation(schema.clone()).into();
        assert_eq!(type_pinned_relation, "pinned_relation".to_string());
        let type_pinned_relation_list: String = FieldType::PinnedRelationList(schema).into();
        assert_eq!(
            type_pinned_relation_list,
            "pinned_relation_list".to_string()
        );
    }

    #[test]
    fn invalid_type_string() {
        assert!("poopy".parse::<FieldType>().is_err());
    }
}
