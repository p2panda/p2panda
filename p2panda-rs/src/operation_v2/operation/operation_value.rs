// SPDX-License-Identifier: AGPL-3.0-or-later

use serde::Serialize;

use crate::document::{DocumentId, DocumentViewId};
use crate::operation_v2::operation::{PinnedRelation, PinnedRelationList, Relation, RelationList};

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

impl From<&[u8]> for OperationValue {
    fn from(value: &[u8]) -> Self {
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

/*#[cfg(test)]
mod tests {
    use rstest::rstest;

    use crate::document::{DocumentId, DocumentViewId};
    use crate::operation_v2::body::{
        OperationId, OperationValue, PinnedRelation, PinnedRelationList, Relation, RelationList,
    };
    use crate::test_utils::fixtures::{document_id, document_view_id, random_operation_id};

    #[rstest]
    fn to_field_type(#[from(random_operation_id)] operation_id: OperationId) {
        let bool = OperationValue::Boolean(true);
        assert_eq!(bool.field_type(), "bool");

        let int = OperationValue::Integer(1);
        assert_eq!(int.field_type(), "int");

        let float = OperationValue::Float(0.1);
        assert_eq!(float.field_type(), "float");

        let text = OperationValue::String("Hello".to_string());
        assert_eq!(text.field_type(), "str");

        let relation = OperationValue::Relation(Relation::new(DocumentId::new(&operation_id)));
        assert_eq!(relation.field_type(), "relation");

        let pinned_relation =
            OperationValue::PinnedRelation(PinnedRelation::new(DocumentViewId::new(&[
                operation_id.clone(),
            ])));
        assert_eq!(pinned_relation.field_type(), "pinned_relation");

        let relation_list =
            OperationValue::RelationList(RelationList::new(vec![DocumentId::new(&operation_id)]));
        assert_eq!(relation_list.field_type(), "relation_list");

        let pinned_relation_list = OperationValue::PinnedRelationList(PinnedRelationList::new(
            vec![DocumentViewId::new(&[operation_id])],
        ));
        assert_eq!(pinned_relation_list.field_type(), "pinned_relation_list");
    }

    #[rstest]
    fn from_primitives(document_id: DocumentId, document_view_id: DocumentViewId) {
        // Scalar types
        assert_eq!(OperationValue::Boolean(true), true.into());
        assert_eq!(OperationValue::Float(1.5), 1.5.into());
        assert_eq!(OperationValue::Integer(3), 3.into());
        assert_eq!(OperationValue::String("hellö".to_string()), "hellö".into());

        // Relation types
        assert_eq!(
            OperationValue::Relation(Relation::new(document_id.clone())),
            document_id.clone().into()
        );
        assert_eq!(
            OperationValue::RelationList(RelationList::new(vec![document_id.clone()])),
            vec![document_id].into()
        );
        assert_eq!(
            OperationValue::PinnedRelation(PinnedRelation::new(document_view_id.clone())),
            document_view_id.clone().into()
        );
        assert_eq!(
            OperationValue::PinnedRelationList(PinnedRelationList::new(vec![
                document_view_id.clone()
            ])),
            vec![document_view_id].into()
        );
    }
}*/
