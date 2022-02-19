// SPDX-License-Identifier: AGPL-3.0-or-later

use std::convert::TryFrom;
use std::str::FromStr;

use crate::document::DocumentView;
use crate::hash::Hash;
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

pub struct SchemaView {
    /// Name of this schema.
    name: String,
    /// Description of this schema.
    description: String,
    /// The fields in this schema.
    fields: Vec<Hash>,
}

pub struct SchemaFieldView {
    /// Name of this schema field.
    name: String,
    /// Type of this schema field.
    field_type: FieldType,
}

/// View onto materialised schema which has fields "name", "description" and "fields".
/// Is validated on being converted from a general DocumentView struct which means so it's inner
/// values can be returned unwrapped by their getter methods.
impl SchemaView {
    /// The name of this schema.
    fn name(&self) -> &str {
        &self.name
    }

    /// The description of this schema.
    fn description(&self) -> &str {
        &self.description
    }

    /// A list of fields assigned to this schema identified by their document id.
    fn fields(&self) -> &[Hash] {
        // Unwrap here because fields were validated on construction
        self.fields.as_slice()
    }
}

/// View onto materialised schema field which has fields "name" and "type".
/// Is validated on being converted from a general DocumentView struct which means so it's inner
/// values can be returned unwrapped by their getter methods.
impl SchemaFieldView {
    /// The name of this schema field.
    fn name(&self) -> &str {
        &self.name
    }

    /// The type of this schema field represented as a FieldType enum variant.
    fn field_type(&self) -> &FieldType {
        &self.field_type
    }
}

impl TryFrom<DocumentView> for SchemaView {
    type Error = SystemSchemaError;

    fn try_from(document_view: DocumentView) -> Result<Self, Self::Error> {
        match document_view.len() {
            len if len < 3 => Err(SystemSchemaError::TooFewFields),
            len if len == 3 => Ok(()),
            _ => Err(SystemSchemaError::TooManyFields),
        }?;

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
            Some(OperationValue::Relation(value)) => Ok(value),
            Some(op) => Err(SystemSchemaError::InvalidField(
                "fields".to_string(),
                op.to_owned(),
            )),
            None => Err(SystemSchemaError::MissingField("fields".to_string())),
        }?;

        Ok(Self {
            name: name.to_string(),
            description: description.to_string(),
            fields: vec![fields.to_owned()],
        })
    }
}

impl TryFrom<DocumentView> for SchemaFieldView {
    type Error = SystemSchemaError;

    fn try_from(document_view: DocumentView) -> Result<Self, Self::Error> {
        match document_view.len() {
            len if len < 2 => Err(SystemSchemaError::TooFewFields),
            len if len == 2 => Ok(()),
            _ => Err(SystemSchemaError::TooManyFields),
        }?;

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
                type_str.parse::<FieldType>()
            }
            Some(op) => Err(SystemSchemaError::InvalidField(
                "type".to_string(),
                op.to_owned(),
            )),
            None => Err(SystemSchemaError::MissingField("type".to_string())),
        }?;

        Ok(Self {
            name: name.to_string(),
            field_type: field_type.to_owned(),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::convert::TryFrom;

    use crate::{
        document::reduce,
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
        let document_view = reduce(&[operation]);
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
        let document_view = reduce(&[bool_field]);
        let field_view = SchemaFieldView::try_from(document_view);
        assert!(field_view.is_ok());
        let field_view = field_view.unwrap();
        assert_eq!(field_view.field_type(), &FieldType::Bool);
        assert_eq!(field_view.name(), "is_accessible");

        let int_field = create_operation(
            schema.clone(),
            fields(vec![
                ("name", OperationValue::Text("capacity".to_string())),
                ("type", OperationValue::Text("int".to_string())),
            ]),
        );
        let document_view = reduce(&[int_field]);
        let field_view = SchemaFieldView::try_from(document_view);
        assert!(field_view.is_ok());
        assert_eq!(field_view.unwrap().field_type(), &FieldType::Int);

        let float_field = create_operation(
            schema.clone(),
            fields(vec![
                ("name", OperationValue::Text("ticket_price".to_string())),
                ("type", OperationValue::Text("float".to_string())),
            ]),
        );
        let document_view = reduce(&[float_field]);
        let field_view = SchemaFieldView::try_from(document_view);
        assert!(field_view.is_ok());
        assert_eq!(field_view.unwrap().field_type(), &FieldType::Float);

        let str_field = create_operation(
            schema.clone(),
            fields(vec![
                ("name", OperationValue::Text("venue_name".to_string())),
                ("type", OperationValue::Text("str".to_string())),
            ]),
        );
        let document_view = reduce(&[str_field]);
        let field_view = SchemaFieldView::try_from(document_view);
        assert!(field_view.is_ok());
        assert_eq!(field_view.unwrap().field_type(), &FieldType::String);

        let relation_field = create_operation(
            schema.clone(),
            fields(vec![
                ("name", OperationValue::Text("address".to_string())),
                ("type", OperationValue::Text("relation".to_string())),
            ]),
        );
        let document_view = reduce(&[relation_field]);
        let field_view = SchemaFieldView::try_from(document_view);
        assert!(field_view.is_ok());
        assert_eq!(field_view.unwrap().field_type(), &FieldType::Relation);

        let invalid_field_type = create_operation(
            schema,
            fields(vec![
                ("name", OperationValue::Text("address".to_string())),
                ("type", OperationValue::Text("hash".to_string())),
            ]),
        );
        let document_view = reduce(&[invalid_field_type]);
        let field_view = SchemaFieldView::try_from(document_view);
        assert!(field_view.is_err());
    }
}
