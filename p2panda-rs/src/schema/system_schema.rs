// SPDX-License-Identifier: AGPL-3.0-or-later
use std::collections::BTreeMap;
use std::convert::TryFrom;

use crate::document::DocumentView;
use crate::operation::OperationValue;

use super::SystemSchemaError;

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

    fn field_type(&self) -> &OperationValue {
        // Unwrap here because fields were validated on construction
        self.0.get("type").unwrap()
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
                Some(OperationValue::Text(_)) if key == "type" => continue,
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
}
