// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::document::DocumentViewId;
use crate::operation_v2::body::error::OperationBuilderError;
use crate::operation_v2::body::plain::PlainFields;
use crate::operation_v2::body::traits::{Actionable, AsBody, Schematic};
use crate::operation_v2::body::validate::validate_body_format;
use crate::operation_v2::body::{
    OperationAction, OperationFields, OperationValue, OperationVersion,
};
use crate::operation_v2::header::Header;
use crate::schema::SchemaId;

#[derive(Clone, Debug)]
pub struct BodyBuilder {
    /// Action of this operation.
    action: OperationAction,

    /// Schema instance of this operation.
    schema_id: SchemaId,

    /// Operation fields.
    fields: Option<OperationFields>,
}

impl BodyBuilder {
    /// Returns a new instance of `BodyBuilder`.
    pub fn new(schema_id: &SchemaId) -> Self {
        Self {
            action: OperationAction::Create,
            schema_id: schema_id.to_owned(),
            fields: None,
        }
    }

    /// Set operation action.
    pub fn action(mut self, action: OperationAction) -> Self {
        self.action = action;
        self
    }

    /// Set operation schema.
    pub fn schema_id(mut self, schema_id: SchemaId) -> Self {
        self.schema_id = schema_id;
        self
    }

    /// Set operation fields.
    pub fn fields(mut self, fields: &[(impl ToString, OperationValue)]) -> Self {
        let mut operation_fields = OperationFields::new();

        for (field_name, field_value) in fields {
            if operation_fields
                .insert(&field_name.to_string(), field_value.to_owned())
                .is_err()
            {
                // Silently fail here as the underlying data type already takes care of duplicates
                // for us ..
            }
        }

        self.fields = Some(operation_fields);
        self
    }

    pub fn build(&self) -> Result<Body, OperationBuilderError> {
        let body = Body {
            action: self.action,
            version: OperationVersion::V1,
            schema_id: self.schema_id.to_owned(),
            fields: self.fields.to_owned(),
        };

        validate_body_format(&body)?;

        Ok(body)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Body {
    /// Version of this operation.
    pub(crate) version: OperationVersion,

    /// Describes if this operation creates, updates or deletes data.
    pub(crate) action: OperationAction,

    /// The id of the schema for this operation.
    pub(crate) schema_id: SchemaId,

    /// Optional fields map holding the operation data.
    pub(crate) fields: Option<OperationFields>,
}

impl AsOperation for Body {
    /// Returns version of operation.
    fn version(&self) -> OperationVersion {
        self.version.to_owned()
    }

    /// Returns action type of operation.
    fn action(&self) -> OperationAction {
        self.action.to_owned()
    }

    /// Returns schema id of operation.
    fn schema_id(&self) -> SchemaId {
        self.schema_id.to_owned()
    }

    /// Returns application data fields of operation.
    fn fields(&self) -> Option<OperationFields> {
        self.fields.clone()
    }
}

impl Actionable for Body {
    fn version(&self) -> OperationVersion {
        self.version
    }

    fn action(&self) -> OperationAction {
        self.action
    }
}

impl Schematic for Body {
    fn schema_id(&self) -> &SchemaId {
        &self.schema_id
    }

    fn fields(&self) -> Option<PlainFields> {
        self.fields.as_ref().map(PlainFields::from)
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use crate::document::DocumentViewId;
    use crate::operation::traits::AsBody;
    use crate::operation::{OperationAction, OperationFields, OperationValue, OperationVersion};
    use crate::schema::SchemaId;
    use crate::test_utils::fixtures::{document_view_id, schema_id};

    use super::BodyBuilder;

    #[rstest]
    fn operation_builder(schema_id: SchemaId, document_view_id: DocumentViewId) {
        let fields = vec![
            ("firstname", "Peter".into()),
            ("lastname", "Panda".into()),
            ("year", 2020.into()),
        ];

        let operation = BodyBuilder::new(&schema_id)
            .action(OperationAction::Update)
            .previous(&document_view_id)
            .fields(&fields)
            .build()
            .unwrap();

        assert_eq!(operation.action(), OperationAction::Update);
        assert_eq!(operation.previous(), Some(document_view_id));
        assert_eq!(operation.fields(), Some(fields.into()));
        assert_eq!(operation.version(), OperationVersion::V1);
        assert_eq!(operation.schema_id(), schema_id);
    }

    #[rstest]
    fn operation_builder_validation(schema_id: SchemaId, document_view_id: DocumentViewId) {
        // Correct CREATE operation
        assert!(BodyBuilder::new(&schema_id)
            .fields(&[("year", 2020.into())])
            .build()
            .is_ok());

        // CREATE operations must not contain previous
        assert!(BodyBuilder::new(&schema_id)
            .action(OperationAction::Create)
            .fields(&[("year", 2020.into())])
            .previous(&document_view_id)
            .build()
            .is_err());

        // CREATE operations must contain fields
        assert!(BodyBuilder::new(&schema_id)
            .action(OperationAction::Create)
            .build()
            .is_err());

        // correct UPDATE operation
        assert!(BodyBuilder::new(&schema_id)
            .action(OperationAction::Update)
            .fields(&[("year", 2020.into())])
            .previous(&document_view_id)
            .build()
            .is_ok());

        // UPDATE operations must have fields
        assert!(BodyBuilder::new(&schema_id)
            .action(OperationAction::Update)
            .previous(&document_view_id)
            .build()
            .is_err());

        // UPDATE operations must have previous
        assert!(BodyBuilder::new(&schema_id)
            .action(OperationAction::Update)
            .fields(&[("year", 2020.into())])
            .build()
            .is_err());

        // correct DELETE operation
        assert!(BodyBuilder::new(&schema_id)
            .action(OperationAction::Delete)
            .previous(&document_view_id)
            .build()
            .is_ok());

        // DELETE operations must not have fields
        assert!(BodyBuilder::new(&schema_id)
            .action(OperationAction::Delete)
            .previous(&document_view_id)
            .fields(&[("year", 2020.into())])
            .build()
            .is_err());

        // DELETE operations must have previous
        assert!(BodyBuilder::new(&schema_id)
            .action(OperationAction::Update)
            .build()
            .is_err());
    }

    #[rstest]
    fn field_ordering(schema_id: SchemaId) {
        // Create first test operation
        let operation_1 = BodyBuilder::new(&schema_id)
            .fields(&[("a", "sloth".into()), ("b", "penguin".into())])
            .build();

        // Create second test operation with same values but different order of fields
        let operation_2 = BodyBuilder::new(&schema_id)
            .fields(&[("b", "penguin".into()), ("a", "sloth".into())])
            .build();

        assert_eq!(operation_1.unwrap(), operation_2.unwrap());
    }

    #[test]
    fn field_iteration() {
        // Create first test operation
        let mut fields = OperationFields::new();
        fields
            .insert("a", OperationValue::String("sloth".to_owned()))
            .unwrap();
        fields
            .insert("b", OperationValue::String("penguin".to_owned()))
            .unwrap();

        let mut field_iterator = fields.iter();

        assert_eq!(
            field_iterator.next().unwrap().1,
            &OperationValue::String("sloth".to_owned())
        );
        assert_eq!(
            field_iterator.next().unwrap().1,
            &OperationValue::String("penguin".to_owned())
        );
    }
}
