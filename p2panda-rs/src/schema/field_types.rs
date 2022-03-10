// SPDX-License-Identifier: AGPL-3.0-or-later

use std::str::FromStr;

use super::FieldTypeError;

/// Valid field types for publishing an application schema.
#[derive(Clone, Debug, Copy, PartialEq)]
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
    Relation,

    /// Defines a [`RelationList`][`crate::operation::RelationList`] field.
    RelationList,

    /// Defines a [`PinnedRelation`][`crate::operation::PinnedRelation`] field.
    PinnedRelation,

    /// Defines a [`PinnedRelationList`][`crate::operation::PinnedRelationList`] field.
    PinnedRelationList,
}

impl FieldType {
    /// Returns the string representation of this type.
    pub fn as_str(&self) -> &str {
        match self {
            FieldType::Bool => "bool",
            FieldType::Int => "int",
            FieldType::Float => "float",
            FieldType::String => "str",
            FieldType::Relation => "relation",
            FieldType::RelationList => "relation_list",
            FieldType::PinnedRelation => "pinned_relation",
            FieldType::PinnedRelationList => "pinned_relation_list",
        }
    }
}

impl FromStr for FieldType {
    type Err = FieldTypeError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "bool" => Ok(FieldType::Bool),
            "int" => Ok(FieldType::Int),
            "float" => Ok(FieldType::Float),
            "str" => Ok(FieldType::String),
            "relation" => Ok(FieldType::Relation),
            "relation_list" => Ok(FieldType::RelationList),
            "pinned_relation" => Ok(FieldType::PinnedRelation),
            "pinned_relation_list" => Ok(FieldType::PinnedRelationList),
            type_str => Err(FieldTypeError::InvalidFieldType(type_str.into())),
        }
    }
}

impl From<FieldType> for String {
    fn from(field_type: FieldType) -> Self {
        field_type.as_str().to_string()
    }
}

#[cfg(test)]
mod tests {

    use crate::schema::FieldType;

    #[test]
    fn serialises() {
        assert_eq!(FieldType::Bool.as_str(), "bool");
        assert_eq!(FieldType::Int.as_str(), "int");
        assert_eq!(FieldType::Float.as_str(), "float");
        assert_eq!(FieldType::String.as_str(), "str");
        assert_eq!(FieldType::Relation.as_str(), "relation");
        assert_eq!(FieldType::RelationList.as_str(), "relation_list");
        assert_eq!(FieldType::PinnedRelation.as_str(), "pinned_relation");
        assert_eq!(
            FieldType::PinnedRelationList.as_str(),
            "pinned_relation_list"
        );
    }
    #[test]
    fn deserialises() {
        assert_eq!(FieldType::Bool, "bool".parse().unwrap());
        assert_eq!(FieldType::Int, "int".parse().unwrap());
        assert_eq!(FieldType::Float, "float".parse().unwrap());
        assert_eq!(FieldType::String, "str".parse().unwrap());
        assert_eq!(FieldType::Relation, "relation".parse().unwrap());
        assert_eq!(FieldType::RelationList, "relation_list".parse().unwrap());
        assert_eq!(
            FieldType::PinnedRelation,
            "pinned_relation".parse().unwrap()
        );
        assert_eq!(
            FieldType::PinnedRelationList,
            "pinned_relation_list".parse().unwrap()
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
        let type_relation: String = FieldType::Relation.into();
        assert_eq!(type_relation, "relation".to_string());
        let type_relation_list: String = FieldType::RelationList.into();
        assert_eq!(type_relation_list, "relation_list".to_string());
        let type_pinned_relation: String = FieldType::PinnedRelation.into();
        assert_eq!(type_pinned_relation, "pinned_relation".to_string());
        let type_pinned_relation_list: String = FieldType::PinnedRelationList.into();
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
