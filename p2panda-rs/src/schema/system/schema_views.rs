// SPDX-License-Identifier: AGPL-3.0-or-later

use std::convert::TryFrom;

use crate::document::{DocumentView, DocumentViewId};
use crate::operation::{OperationValue, OperationValueRelationList, PinnedRelationList};
use crate::schema::FieldType;

use super::SystemSchemaError;

/// View onto materialised schema which has fields "name", "description" and "fields".
///
/// The fields are validated when converting a DocumentView struct into this type.
#[derive(Clone, Debug, PartialEq)]
pub struct SchemaView {
    /// ID of this schema view.
    id: DocumentViewId,

    /// Name of this schema.
    name: String,

    /// Description of this schema.
    description: String,

    /// The fields in this schema.
    fields: PinnedRelationList,
}

#[allow(dead_code)] // These methods aren't used yet...
impl SchemaView {
    /// The id of this schema view.
    pub fn view_id(&self) -> &DocumentViewId {
        &self.id
    }

    /// The name of this schema.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The description of this schema.
    pub fn description(&self) -> &str {
        &self.description
    }

    /// A list of fields assigned to this schema identified by their document id.
    pub fn fields(&self) -> &PinnedRelationList {
        &self.fields
    }
}

impl TryFrom<DocumentView> for SchemaView {
    type Error = SystemSchemaError;

    fn try_from(document_view: DocumentView) -> Result<Self, Self::Error> {
        let name = match document_view.get("name") {
            Some(OperationValue::Text(value)) => Ok(value),
            Some(op) => Err(SystemSchemaError::InvalidField(
                "name".to_string(),
                op.to_owned(),
            )),
            None => Err(SystemSchemaError::MissingField("name".to_string())),
        }?;

        let description = match document_view.get("description") {
            Some(OperationValue::Text(value)) => Ok(value),
            Some(op) => Err(SystemSchemaError::InvalidField(
                "description".to_string(),
                op.to_owned(),
            )),
            None => Err(SystemSchemaError::MissingField("description".to_string())),
        }?;

        let fields = match document_view.get("fields") {
            Some(OperationValue::RelationList(OperationValueRelationList::Pinned(value))) => {
                Ok(value)
            }
            Some(op) => Err(SystemSchemaError::InvalidField(
                "fields".to_string(),
                op.to_owned(),
            )),
            None => Err(SystemSchemaError::MissingField("fields".to_string())),
        }?;

        Ok(Self {
            id: document_view.id().clone(),
            name: name.to_string(),
            description: description.to_string(),
            fields: fields.to_owned(),
        })
    }
}

/// View onto materialised schema field which has fields "name" and "type".
///
/// The fields are validated when converting a DocumentView struct into this type.
#[derive(Clone, Debug, PartialEq)]
pub struct SchemaFieldView {
    // Identifier of this schema field view.
    id: DocumentViewId,

    /// Name of this schema field.
    name: String,

    /// Type of this schema field.
    field_type: FieldType,
}

#[allow(dead_code)] // These methods aren't used yet...
impl SchemaFieldView {
    /// The id of this schema view.
    pub fn id(&self) -> &DocumentViewId {
        &self.id
    }

    /// The name of this schema field.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The type of this schema field represented as a FieldType enum variant.
    pub fn field_type(&self) -> &FieldType {
        &self.field_type
    }
}

impl TryFrom<DocumentView> for SchemaFieldView {
    type Error = SystemSchemaError;

    fn try_from(document_view: DocumentView) -> Result<Self, Self::Error> {
        let name = match document_view.get("name") {
            Some(OperationValue::Text(value)) => Ok(value),
            Some(op) => Err(SystemSchemaError::InvalidField(
                "name".to_string(),
                op.to_owned(),
            )),
            None => Err(SystemSchemaError::MissingField("name".to_string())),
        }?;

        let field_type = match document_view.get("type") {
            Some(OperationValue::Text(type_str)) => {
                // Validate the type string parses into a FieldType
                Ok(type_str.parse::<FieldType>()?)
            }
            Some(op) => Err(SystemSchemaError::InvalidField(
                "type".to_string(),
                op.to_owned(),
            )),
            None => Err(SystemSchemaError::MissingField("type".to_string())),
        }?;

        Ok(Self {
            id: document_view.id().clone(),
            name: name.to_string(),
            field_type: field_type.to_owned(),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::convert::TryFrom;

    use rstest::rstest;

    use crate::document::{DocumentView, DocumentViewId};
    use crate::hash::Hash;
    use crate::operation::{OperationValue, OperationValueRelationList, PinnedRelationList};
    use crate::schema::system::SchemaFieldView;
    use crate::test_utils::fixtures::random_hash;

    use super::{FieldType, SchemaView};

    #[rstest]
    fn from_document_view(
        #[from(random_hash)] relation_operation_id: Hash,
        #[from(random_hash)] view_id: Hash,
    ) {
        let mut bool_field = BTreeMap::new();
        bool_field.insert(
            "name".to_string(),
            OperationValue::Text("venue_name".to_string()),
        );
        bool_field.insert(
            "description".to_string(),
            OperationValue::Text("Describes a venue".to_string()),
        );
        bool_field.insert(
            "fields".to_string(),
            OperationValue::RelationList(OperationValueRelationList::Pinned(
                PinnedRelationList::new(vec![DocumentViewId::new(vec![relation_operation_id])]),
            )),
        );

        let document_view_id = DocumentViewId::new(vec![view_id]);
        let document_view = DocumentView::new(document_view_id, bool_field);

        assert!(SchemaView::try_from(document_view).is_ok());
    }

    #[rstest]
    fn field_type_from_document_view(#[from(random_hash)] view_id: Hash) {
        // Prepare common document view id
        let document_view_id = DocumentViewId::new(vec![view_id]);

        // Create first schema field "is_accessible"
        // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

        let mut bool_field = BTreeMap::new();
        bool_field.insert(
            "name".to_string(),
            OperationValue::Text("is_accessible".to_string()),
        );
        bool_field.insert(
            "type".to_string(),
            OperationValue::Text(FieldType::Bool.into()),
        );

        let document_view = DocumentView::new(document_view_id.clone(), bool_field);
        let field_view = SchemaFieldView::try_from(document_view);
        assert!(field_view.is_ok());

        let field_view = field_view.unwrap();
        assert_eq!(field_view.field_type(), &FieldType::Bool);
        assert_eq!(field_view.name(), "is_accessible");

        // Create second schema field "capacity"
        // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

        let mut capacity_field = BTreeMap::new();
        capacity_field.insert(
            "name".to_string(),
            OperationValue::Text("capacity".to_string()),
        );
        capacity_field.insert(
            "type".to_string(),
            OperationValue::Text(FieldType::Int.into()),
        );

        let document_view = DocumentView::new(document_view_id.clone(), capacity_field);
        let field_view = SchemaFieldView::try_from(document_view);
        assert!(field_view.is_ok());
        assert_eq!(field_view.unwrap().field_type(), &FieldType::Int);

        // Create third schema field "ticket_price"
        // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

        let mut float_field = BTreeMap::new();
        float_field.insert(
            "name".to_string(),
            OperationValue::Text("ticket_price".to_string()),
        );
        float_field.insert(
            "type".to_string(),
            OperationValue::Text(FieldType::Float.into()),
        );

        let document_view = DocumentView::new(document_view_id.clone(), float_field);
        let field_view = SchemaFieldView::try_from(document_view);
        assert!(field_view.is_ok());
        assert_eq!(field_view.unwrap().field_type(), &FieldType::Float);

        // Create fourth schema field "venue_name"
        // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

        let mut str_field = BTreeMap::new();
        str_field.insert(
            "name".to_string(),
            OperationValue::Text("venue_name".to_string()),
        );
        str_field.insert(
            "type".to_string(),
            OperationValue::Text(FieldType::String.into()),
        );

        let document_view = DocumentView::new(document_view_id.clone(), str_field);
        let field_view = SchemaFieldView::try_from(document_view);
        assert!(field_view.is_ok());
        assert_eq!(field_view.unwrap().field_type(), &FieldType::String);

        // Create fifth schema field "address"
        // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

        let mut relation_field = BTreeMap::new();
        relation_field.insert(
            "name".to_string(),
            OperationValue::Text("address".to_string()),
        );
        relation_field.insert(
            "type".to_string(),
            OperationValue::Text(FieldType::Relation.into()),
        );

        let document_view = DocumentView::new(document_view_id, relation_field);
        let field_view = SchemaFieldView::try_from(document_view);
        assert!(field_view.is_ok());
        assert_eq!(field_view.unwrap().field_type(), &FieldType::Relation);
    }

    #[rstest]
    fn invalid_schema_field(#[from(random_hash)] view_id: Hash) {
        let document_view_id = DocumentViewId::new(vec![view_id]);

        let mut invalid_field = BTreeMap::new();
        invalid_field.insert(
            "name".to_string(),
            OperationValue::Text("address".to_string()),
        );
        invalid_field.insert("type".to_string(), OperationValue::Text("hash".to_string()));

        let document_view = DocumentView::new(document_view_id, invalid_field);
        let field_view = SchemaFieldView::try_from(document_view);
        assert!(field_view.is_err());
    }
}
