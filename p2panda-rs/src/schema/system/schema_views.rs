// SPDX-License-Identifier: AGPL-3.0-or-later

use std::convert::TryFrom;

use crate::document::{DocumentView, DocumentViewId, DocumentViewValue};
use crate::operation::{OperationValue, PinnedRelationList};
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
            Some(document_view_value) => {
                if let DocumentViewValue::Value(_, OperationValue::Text(value)) =
                    document_view_value
                {
                    Ok(value)
                } else {
                    Err(SystemSchemaError::InvalidField(
                        "name".into(),
                        document_view_value.clone(),
                    ))
                }
            }
            None => Err(SystemSchemaError::MissingField("name".into())),
        }?;

        let description = match document_view.get("description") {
            Some(document_view_value) => {
                if let DocumentViewValue::Value(_, OperationValue::Text(value)) =
                    document_view_value
                {
                    Ok(value)
                } else {
                    Err(SystemSchemaError::InvalidField(
                        "description".into(),
                        document_view_value.clone(),
                    ))
                }
            }
            None => Err(SystemSchemaError::MissingField("description".into())),
        }?;

        let fields = match document_view.get("fields") {
            Some(document_view_value) => {
                if let DocumentViewValue::Value(_, OperationValue::PinnedRelationList(value)) =
                    document_view_value
                {
                    Ok(value)
                } else {
                    Err(SystemSchemaError::InvalidField(
                        "fields".into(),
                        document_view_value.clone(),
                    ))
                }
            }
            None => Err(SystemSchemaError::MissingField("fields".into())),
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
            Some(document_view_value) => {
                if let DocumentViewValue::Value(_, OperationValue::Text(value)) =
                    document_view_value
                {
                    Ok(value)
                } else {
                    Err(SystemSchemaError::InvalidField(
                        "name".into(),
                        document_view_value.clone(),
                    ))
                }
            }
            None => Err(SystemSchemaError::MissingField("name".into())),
        }?;

        let field_type = match document_view.get("type") {
            Some(document_view_value) => {
                if let DocumentViewValue::Value(_, OperationValue::Text(type_str)) =
                    document_view_value
                {
                    // Validate the type string parses into a FieldType
                    Ok(type_str.parse::<FieldType>()?)
                } else {
                    Err(SystemSchemaError::InvalidField(
                        "type".into(),
                        document_view_value.to_owned(),
                    ))
                }
            }
            None => Err(SystemSchemaError::MissingField("type".to_string())),
        }?;

        Ok(Self {
            id: document_view.id().clone(),
            name: name.to_string(),
            field_type,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::convert::TryFrom;

    use rstest::rstest;

    use crate::document::{DocumentView, DocumentViewFields, DocumentViewId, DocumentViewValue};
    use crate::operation::{OperationId, OperationValue, PinnedRelationList};
    use crate::schema::system::SchemaFieldView;
    use crate::schema::SchemaId;
    use crate::test_utils::fixtures::{document_view_id, random_operation_id, schema};

    use super::{FieldType, SchemaView};

    #[rstest]
    fn from_document_view(
        #[from(random_operation_id)] operation_id: OperationId,
        #[from(random_operation_id)] relation: OperationId,
        #[from(random_operation_id)] view_id: OperationId,
    ) {
        let mut venue_schema = DocumentViewFields::new();
        venue_schema.insert(
            "name",
            DocumentViewValue::Value(
                operation_id.clone(),
                OperationValue::Text("venue_name".to_string()),
            ),
        );
        venue_schema.insert(
            "description",
            DocumentViewValue::Value(
                operation_id.clone(),
                OperationValue::Text("Describes a venue".to_string()),
            ),
        );
        venue_schema.insert(
            "fields",
            DocumentViewValue::Value(
                operation_id,
                OperationValue::PinnedRelationList(PinnedRelationList::new(vec![
                    DocumentViewId::new(&[relation]),
                ])),
            ),
        );
        let document_view_id = DocumentViewId::from(view_id);
        let document_view = DocumentView::new(document_view_id, venue_schema);

        assert!(SchemaView::try_from(document_view).is_ok());
    }

    #[rstest]
    fn field_type_from_document_view(
        #[from(random_operation_id)] operation_id: OperationId,
        document_view_id: DocumentViewId,
        #[from(schema)] address_schema: SchemaId,
    ) {
        // Create first schema field "is_accessible"
        // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

        let mut bool_field = DocumentViewFields::new();
        bool_field.insert(
            "name",
            DocumentViewValue::Value(
                operation_id.clone(),
                OperationValue::Text("is_accessible".to_string()),
            ),
        );
        bool_field.insert(
            "type",
            DocumentViewValue::Value(operation_id.clone(), FieldType::Bool.into()),
        );

        let document_view = DocumentView::new(document_view_id.clone(), bool_field);
        let field_view = SchemaFieldView::try_from(document_view);
        assert!(field_view.is_ok());

        let field_view = field_view.unwrap();
        assert_eq!(field_view.field_type(), &FieldType::Bool);
        assert_eq!(field_view.name(), "is_accessible");

        // Create second schema field "capacity"
        // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

        let mut capacity_field = DocumentViewFields::new();
        capacity_field.insert(
            "name",
            DocumentViewValue::Value(
                operation_id.clone(),
                OperationValue::Text("capacity".to_string()),
            ),
        );
        capacity_field.insert(
            "type",
            DocumentViewValue::Value(operation_id.clone(), FieldType::Int.into()),
        );

        let document_view = DocumentView::new(document_view_id.clone(), capacity_field);
        let field_view = SchemaFieldView::try_from(document_view);
        assert!(field_view.is_ok());
        assert_eq!(field_view.unwrap().field_type(), &FieldType::Int);

        // Create third schema field "ticket_price"
        // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

        let mut float_field = DocumentViewFields::new();
        float_field.insert(
            "name",
            DocumentViewValue::Value(
                operation_id.clone(),
                OperationValue::Text("ticket_price".to_string()),
            ),
        );
        float_field.insert(
            "type",
            DocumentViewValue::Value(operation_id.clone(), FieldType::Float.into()),
        );

        let document_view = DocumentView::new(document_view_id.clone(), float_field);
        let field_view = SchemaFieldView::try_from(document_view);
        assert!(field_view.is_ok());
        assert_eq!(field_view.unwrap().field_type(), &FieldType::Float);

        // Create fourth schema field "venue_name"
        // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

        let mut str_field = DocumentViewFields::new();
        str_field.insert(
            "name",
            DocumentViewValue::Value(
                operation_id.clone(),
                OperationValue::Text("venue_name".to_string()),
            ),
        );
        str_field.insert(
            "type",
            DocumentViewValue::Value(operation_id.clone(), FieldType::String.into()),
        );

        let document_view = DocumentView::new(document_view_id.clone(), str_field);
        let field_view = SchemaFieldView::try_from(document_view);
        assert!(field_view.is_ok());
        assert_eq!(field_view.unwrap().field_type(), &FieldType::String);

        // Create fifth schema field "address"
        // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

        let mut relation_field = DocumentViewFields::new();
        relation_field.insert(
            "name",
            DocumentViewValue::Value(
                operation_id.clone(),
                OperationValue::Text("address".to_string()),
            ),
        );
        relation_field.insert(
            "type",
            DocumentViewValue::Value(
                operation_id.clone(),
                FieldType::Relation(address_schema.clone()).into(),
            ),
        );

        let document_view = DocumentView::new(document_view_id, relation_field);
        let field_view = SchemaFieldView::try_from(document_view);
        assert!(field_view.is_ok());
        assert_eq!(
            field_view.unwrap().field_type(),
            &FieldType::Relation(address_schema)
        );
    }

    #[rstest]
    fn invalid_schema_field(
        #[from(random_operation_id)] operation_id: OperationId,
        document_view_id: DocumentViewId,
    ) {
        let mut invalid_field = DocumentViewFields::new();
        invalid_field.insert(
            "name",
            DocumentViewValue::Value(
                operation_id.clone(),
                OperationValue::Text("address".to_string()),
            ),
        );
        invalid_field.insert(
            "type",
            DocumentViewValue::Value(operation_id, OperationValue::Text("hash".to_string())),
        );

        let document_view = DocumentView::new(document_view_id, invalid_field);
        let field_view = SchemaFieldView::try_from(document_view);
        assert!(field_view.is_err());
    }
}
