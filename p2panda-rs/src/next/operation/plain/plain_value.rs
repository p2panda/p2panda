// SPDX-License-Identifier: AGPL-3.0-or-later

use serde::{Deserialize, Serialize};

/// Operation field values which have not been checked against a schema yet.
///
/// This enum expresses some operation field types as groups, since "String" or "Relation" are
/// represented by the same internal data type (a simple string).
///
/// Latest when combining the plain values with a schema, the inner types, especially the
/// relations, get checked against their correct format.
#[derive(Deserialize, Serialize, Debug, PartialEq, Clone)]
#[serde(untagged)]
pub enum PlainValue {
    /// Boolean value.
    Boolean(bool),

    /// Integer value.
    Integer(i64),

    /// Float value.
    Float(f64),

    /// String value which can be either a text or relation (document id).
    StringOrRelation(String),

    /// List of strings which can either be a pinned relation (list of operation ids) or a relation
    /// list (list of document ids).
    PinnedRelationOrRelationList(Vec<String>),

    /// List of a list of strings which is a pinned relation list.
    PinnedRelationList(Vec<Vec<String>>),
}

impl PlainValue {
    /// Returns the string representation of these plain values.
    ///
    /// This is useful for composing error messages or debug logs.
    pub fn field_type(&self) -> &str {
        match self {
            PlainValue::Boolean(_) => "bool",
            PlainValue::Integer(_) => "int",
            PlainValue::Float(_) => "float",
            PlainValue::StringOrRelation(_) => "str",
            PlainValue::PinnedRelationOrRelationList(_) => "str[]",
            PlainValue::PinnedRelationList(_) => "str[][]",
        }
    }
}
