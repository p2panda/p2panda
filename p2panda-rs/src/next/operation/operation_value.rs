// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::next::operation::{PinnedRelation, PinnedRelationList, Relation, RelationList};

/// Enum of possible data types which can be added to the operations fields as values.
#[derive(Clone, Debug, PartialEq)]
pub enum OperationValue {
    /// Boolean value.
    Boolean(bool),

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

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use crate::next::document::{DocumentId, DocumentViewId};
    use crate::next::operation::{
        OperationId, PinnedRelation, PinnedRelationList, Relation, RelationList,
    };
    use crate::next::schema::FieldType;
    use crate::next::test_utils::fixtures::{random_document_id, random_operation_id};
    use crate::Validate;

    use super::OperationValue;

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

        let relation =
            OperationValue::Relation(Relation::new(DocumentId::new(operation_id.clone())));
        assert_eq!(relation.field_type(), "relation");

        let pinned_relation = OperationValue::PinnedRelation(PinnedRelation::new(
            DocumentViewId::new(&[operation_id.clone()]).unwrap(),
        ));
        assert_eq!(pinned_relation.field_type(), "pinned_relation");

        let relation_list = OperationValue::RelationList(RelationList::new(vec![DocumentId::new(
            operation_id.clone(),
        )]));
        assert_eq!(relation_list.field_type(), "relation_list");

        let pinned_relation_list = OperationValue::PinnedRelationList(PinnedRelationList::new(
            vec![DocumentViewId::new(&[operation_id]).unwrap()],
        ));
        assert_eq!(pinned_relation_list.field_type(), "pinned_relation_list");
    }

    // @TODO: This is not really possible without schemas anymore, could this be moved to operation
    // decoding and validation?
    /* #[rstest]
    #[allow(clippy::too_many_arguments)]
    fn encode_decode_relations(
        #[from(random_operation_id)] operation_1: OperationId,
        #[from(random_operation_id)] operation_2: OperationId,
        #[from(random_operation_id)] operation_3: OperationId,
        #[from(random_operation_id)] operation_4: OperationId,
        #[from(random_operation_id)] operation_5: OperationId,
        #[from(random_operation_id)] operation_6: OperationId,
        #[from(random_operation_id)] operation_7: OperationId,
        #[from(random_operation_id)] operation_8: OperationId,
    ) {
        // 1. Unpinned relation
        let relation = OperationValue::Relation(Relation::new(DocumentId::new(operation_1)));
        assert_eq!(
            relation,
            OperationValue::deserialize_str(&relation.serialize())
        );

        // 2. Pinned relation
        let pinned_relation = OperationValue::PinnedRelation(PinnedRelation::new(
            DocumentViewId::new(&[operation_2, operation_3]).unwrap(),
        ));
        assert_eq!(
            pinned_relation,
            OperationValue::deserialize_str(&pinned_relation.serialize())
        );

        // 3. Unpinned relation list
        let relation_list = OperationValue::RelationList(RelationList::new(vec![
            DocumentId::new(operation_4),
            DocumentId::new(operation_5),
        ]));
        assert_eq!(
            relation_list,
            OperationValue::deserialize_str(&relation_list.serialize())
        );

        // 4. Pinned relation list
        let pinned_relation_list =
            OperationValue::PinnedRelationList(PinnedRelationList::new(vec![
                DocumentViewId::new(&[operation_6, operation_7]).unwrap(),
                DocumentViewId::new(&[operation_8]).unwrap(),
            ]));
        assert_eq!(
            pinned_relation_list,
            OperationValue::deserialize_str(&pinned_relation_list.serialize())
        );
    } */

    // @TODO: This is not really possible without schemas anymore, could this be moved to operation
    // decoding and validation?
    /* #[rstest]
    fn validation_ok(
        #[from(random_document_id)] document_1: DocumentId,
        #[from(random_document_id)] document_2: DocumentId,
        #[from(random_operation_id)] operation_id_1: OperationId,
        #[from(random_operation_id)] operation_id_2: OperationId,
    ) {
        let relation = Relation::new(document_1.clone());
        let value = OperationValue::Relation(relation);
        assert!(value.validate().is_ok());

        let pinned_relation = PinnedRelation::new(
            DocumentViewId::new(&[operation_id_1.clone(), operation_id_2.clone()]).unwrap(),
        );
        let value = OperationValue::PinnedRelation(pinned_relation);
        assert!(value.validate().is_ok());

        let relation_list = RelationList::new(vec![document_1, document_2]);
        let value = OperationValue::RelationList(relation_list);
        assert!(value.validate().is_ok());

        let pinned_relation_list = PinnedRelationList::new(vec![
            DocumentViewId::from(operation_id_1),
            DocumentViewId::from(operation_id_2),
        ]);
        let value = OperationValue::PinnedRelationList(pinned_relation_list);
        assert!(value.validate().is_ok());
    } */

    // @TODO: This is not really possible without schemas anymore, could this be moved to operation
    // decoding and validation?
    /* #[test]
    fn validation_invalid_relations() {
        // "relation_list" operation value with invalid hash:
        //
        // {
        //  "type": "relation_list",
        //  "value": ["This is not a hash"]
        // }
        let invalid_hash = "A264747970656D72656C6174696F6E5F6C6973746576616C7565817254686973206973206E6F7420612068617368";
        let value: OperationValue = OperationValue::deserialize_str(invalid_hash);
        assert!(value.validate().is_err());

        // "relation" operation value with invalid hash:
        //
        // {
        //  "type": "relation",
        //  "value": "This is not a hash"
        // }
        let invalid_hash =
            "A264747970656872656C6174696F6E6576616C75657254686973206973206E6F7420612068617368";
        let value: OperationValue = OperationValue::deserialize_str(invalid_hash);
        assert!(value.validate().is_err());
    } */

    // @TODO: This is not really possible without schemas anymore, could this be moved to operation
    // decoding and validation?
    /* #[test]
    fn validation_relation_lists_can_be_empty() {
        let pinned_relation_list = PinnedRelationList::new(vec![]);
        let value = OperationValue::PinnedRelationList(pinned_relation_list);
        assert!(value.validate().is_ok());

        let relation_list = RelationList::new(vec![]);
        let value = OperationValue::RelationList(relation_list);
        assert!(value.validate().is_ok());
    } */
}