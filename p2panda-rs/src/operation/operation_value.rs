// SPDX-License-Identifier: AGPL-3.0-or-later

use serde::Serialize;

use crate::document::{DocumentId, DocumentViewId};
use crate::operation::{PinnedRelation, PinnedRelationList, Relation, RelationList};

/// Enum of possible data types which can be added to the operations fields as values.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub enum OperationValue {
    /// Boolean value.
    Boolean(bool),

    /// Bytes value.
    #[serde(with = "serde_bytes")]
    Bytes(Vec<u8>),

    /// Signed integer value.
    Integer(i64),

    /// Floating point value.
    Float(f64),

    /// String value.
    String(String),

    /// Reference to a document.
    Relation(Relation),

    /// Reference to a list of documents.
    RelationList(RelationList),

    /// Reference to a document view.
    PinnedRelation(PinnedRelation),

    /// Reference to a list of document views.
    PinnedRelationList(PinnedRelationList),
}

impl OperationValue {
    /// Return the field type for this operation value as a string
    pub fn field_type(&self) -> &str {
        match self {
            OperationValue::Boolean(_) => "bool",
            OperationValue::Bytes(_) => "bytes",
            OperationValue::Integer(_) => "int",
            OperationValue::Float(_) => "float",
            OperationValue::String(_) => "str",
            OperationValue::Relation(_) => "relation",
            OperationValue::RelationList(_) => "relation_list",
            OperationValue::PinnedRelation(_) => "pinned_relation",
            OperationValue::PinnedRelationList(_) => "pinned_relation_list",
        }
    }
}

impl From<bool> for OperationValue {
    fn from(value: bool) -> Self {
        OperationValue::Boolean(value)
    }
}

impl From<f64> for OperationValue {
    fn from(value: f64) -> Self {
        OperationValue::Float(value)
    }
}

impl From<i64> for OperationValue {
    fn from(value: i64) -> Self {
        OperationValue::Integer(value)
    }
}

impl From<String> for OperationValue {
    fn from(value: String) -> Self {
        OperationValue::String(value)
    }
}

impl From<&str> for OperationValue {
    fn from(value: &str) -> Self {
        OperationValue::String(value.to_string())
    }
}

impl From<Vec<u8>> for OperationValue {
    fn from(value: Vec<u8>) -> Self {
        OperationValue::Bytes(value.to_owned())
    }
}

impl From<DocumentId> for OperationValue {
    fn from(value: DocumentId) -> Self {
        OperationValue::Relation(Relation::new(value))
    }
}

impl From<Vec<DocumentId>> for OperationValue {
    fn from(value: Vec<DocumentId>) -> Self {
        OperationValue::RelationList(RelationList::new(value))
    }
}

impl From<DocumentViewId> for OperationValue {
    fn from(value: DocumentViewId) -> Self {
        OperationValue::PinnedRelation(PinnedRelation::new(value))
    }
}

impl From<Vec<DocumentViewId>> for OperationValue {
    fn from(value: Vec<DocumentViewId>) -> Self {
        OperationValue::PinnedRelationList(PinnedRelationList::new(value))
    }
}
