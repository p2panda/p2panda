// SPDX-License-Identifier: AGPL-3.0-or-later
use std::collections::BTreeMap;
use std::convert::TryFrom;
use std::str::FromStr;

use crate::document::DocumentView;
use crate::operation::OperationValue;

use super::SystemSchemaError;

#[derive(Clone, Debug, Copy, PartialEq)]
#[allow(missing_docs)]
pub enum FieldType {
    Bool,
    Int,
    Float,
    String,
    Relation,
}

impl FromStr for FieldType {
    type Err = SystemSchemaError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "bool" => Ok(FieldType::Bool),
            "int" => Ok(FieldType::Int),
            "float" => Ok(FieldType::Float),
            "str" => Ok(FieldType::String),
            "relation" => Ok(FieldType::Relation),
            _ => Err(SystemSchemaError::InvalidFieldType),
        }
    }
}

struct SchemaView(DocumentView);
struct SchemaFieldView(DocumentView);

impl SchemaView {
    fn name(&self) -> &OperationValue {
        // Unwrap here because fields were validated on construction
        self.0.get("name").unwrap()
    }

    fn description(&self) -> &OperationValue {
        // Unwrap here because fields were validated on construction
        self.0.get("description").unwrap()
    }

    fn fields(&self) -> &OperationValue {
        // Unwrap here because fields were validated on construction
        self.0.get("fields").unwrap()
    }
}

impl SchemaFieldView {
    pub fn fields(&self) -> BTreeMap<String, OperationValue> {
        self.0.clone().into()
    }
}

impl SchemaFieldView {
    fn name(&self) -> &OperationValue {
        // Unwrap here because fields were validated on construction
        self.0.get("name").unwrap()
    }

    fn field_type(&self) -> FieldType {
        // Unwrap here because fields were validated on construction
        self.0
            .get("type")
            .and_then(|value| match value {
                OperationValue::Text(type_str) => Some(type_str.parse::<FieldType>().unwrap()),
                _ => None,
            })
            .unwrap()
    }
}

impl TryFrom<DocumentView> for SchemaView {
    type Error = SystemSchemaError;

    fn try_from(document_view: DocumentView) -> Result<Self, Self::Error> {
        let mut fields = vec!["name", "description", "fields"];
        let fields_len = fields.len();

        match document_view.len() {
            len if len < fields_len => Err(SystemSchemaError::TooFewFields),
            len if len == fields_len => Ok(()),
            _ => Err(SystemSchemaError::TooManyFields),
        }?;

        while let Some(key) = fields.pop() {
            match document_view.get(key) {
                Some(OperationValue::Text(_)) if key == "name" => continue,
                Some(OperationValue::Text(_)) if key == "description" => continue,
                // This will be replaced with new relation-list type
                Some(OperationValue::Relation(_)) if key == "fields" => continue,
                Some(op) => {
                    return Err(SystemSchemaError::InvalidField(
                        key.to_string(),
                        op.to_owned(),
                    ))
                }
                None => return Err(SystemSchemaError::MissingField(key.to_string())),
            }
        }

        Ok(Self(document_view))
    }
}

impl TryFrom<DocumentView> for SchemaFieldView {
    type Error = SystemSchemaError;

    fn try_from(document_view: DocumentView) -> Result<Self, Self::Error> {
        let mut fields = vec!["name", "type"];
        let fields_len = fields.len();

        match document_view.len() {
            len if len < fields_len => Err(SystemSchemaError::TooFewFields),
            len if len == fields_len => Ok(()),
            _ => Err(SystemSchemaError::TooManyFields),
        }?;

        while let Some(key) = fields.pop() {
            match document_view.get(key) {
                Some(OperationValue::Text(_)) if key == "name" => continue,
                Some(OperationValue::Text(type_str)) if key == "type" => {
                    // Validate the type string parses into a FieldType
                    type_str.parse::<FieldType>()?;
                    continue;
                }
                Some(op) => {
                    return Err(SystemSchemaError::InvalidField(
                        key.to_string(),
                        op.to_owned(),
                    ))
                }
                None => return Err(SystemSchemaError::MissingField(key.to_string())),
            }
        }

        Ok(Self(document_view))
    }
}

#[cfg(test)]
mod tests {
    use std::convert::{TryFrom, TryInto};

    use crate::{
        document::DocumentView,
        hash::Hash,
        operation::OperationValue,
        schema::system_schema::{FieldType, SchemaFieldView},
        test_utils::fixtures::{create_operation, fields, hash, schema},
    };
    use rstest::rstest;

    use super::SchemaView;

    #[rstest]
    fn from_document_view(#[from(hash)] relation_hash: Hash, schema: Hash) {
        let operation = create_operation(
            schema,
            fields(vec![
                ("name", OperationValue::Text("venue_name".to_string())),
                (
                    "description",
                    OperationValue::Text("Describes a venue".to_string()),
                ),
                ("fields", OperationValue::Relation(relation_hash)),
            ]),
        );
        let document_view: DocumentView = operation.try_into().unwrap();
        assert!(SchemaView::try_from(document_view).is_ok());
    }

    #[rstest]
    fn field_type_from_document_view(schema: Hash) {
        let bool_field = create_operation(
            schema.clone(),
            fields(vec![
                ("name", OperationValue::Text("is_accessible".to_string())),
                ("type", OperationValue::Text("bool".to_string())),
            ]),
        );
        let document_view: DocumentView = bool_field.try_into().unwrap();
        let field_view = SchemaFieldView::try_from(document_view);
        assert!(field_view.is_ok());
        let field_view = field_view.unwrap();
        assert_eq!(field_view.field_type(), FieldType::Bool);
        assert_eq!(
            field_view.name(),
            &OperationValue::Text("is_accessible".to_string())
        );

        let int_field = create_operation(
            schema.clone(),
            fields(vec![
                ("name", OperationValue::Text("capacity".to_string())),
                ("type", OperationValue::Text("int".to_string())),
            ]),
        );
        let document_view: DocumentView = int_field.try_into().unwrap();
        let field_view = SchemaFieldView::try_from(document_view);
        assert!(field_view.is_ok());
        assert_eq!(field_view.unwrap().field_type(), FieldType::Int);

        let float_field = create_operation(
            schema.clone(),
            fields(vec![
                ("name", OperationValue::Text("ticket_price".to_string())),
                ("type", OperationValue::Text("float".to_string())),
            ]),
        );
        let document_view: DocumentView = float_field.try_into().unwrap();
        let field_view = SchemaFieldView::try_from(document_view);
        assert!(field_view.is_ok());
        assert_eq!(field_view.unwrap().field_type(), FieldType::Float);

        let str_field = create_operation(
            schema.clone(),
            fields(vec![
                ("name", OperationValue::Text("venue_name".to_string())),
                ("type", OperationValue::Text("str".to_string())),
            ]),
        );
        let document_view: DocumentView = str_field.try_into().unwrap();
        let field_view = SchemaFieldView::try_from(document_view);
        assert!(field_view.is_ok());
        assert_eq!(field_view.unwrap().field_type(), FieldType::String);

        let relation_field = create_operation(
            schema.clone(),
            fields(vec![
                ("name", OperationValue::Text("address".to_string())),
                ("type", OperationValue::Text("relation".to_string())),
            ]),
        );
        let document_view: DocumentView = relation_field.try_into().unwrap();
        let field_view = SchemaFieldView::try_from(document_view);
        assert!(field_view.is_ok());
        assert_eq!(field_view.unwrap().field_type(), FieldType::Relation);

        let invalid_field_type = create_operation(
            schema.clone(),
            fields(vec![
                ("name", OperationValue::Text("address".to_string())),
                ("type", OperationValue::Text("hash".to_string())),
            ]),
        );
        let document_view: DocumentView = invalid_field_type.try_into().unwrap();
        let field_view = SchemaFieldView::try_from(document_view);
        assert!(field_view.is_err());
    }
}
