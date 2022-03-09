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
