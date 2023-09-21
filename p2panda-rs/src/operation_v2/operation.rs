// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::document::DocumentViewId;
use crate::operation_v2::body::error::OperationBuilderError;
use crate::operation_v2::body::plain::PlainFields;
use crate::operation_v2::body::validate::validate_operation_format;
use crate::operation_v2::body::{
    OperationAction, OperationFields, OperationValue, OperationVersion,
};
use crate::operation_v2::traits::{Actionable, AsOperation, Schematic};
use crate::schema::SchemaId;

pub struct Operation {
    /// Version of this operation.
    pub(crate) version: OperationVersion,

    /// Describes if this operation creates, updates or deletes data.
    pub(crate) action: OperationAction,

    /// The id of the schema for this operation.
    pub(crate) schema_id: SchemaId,

    /// Optional document view id containing the operation ids directly preceding this one in the
    /// document.
    pub(crate) previous: Option<DocumentViewId>,

    /// Optional fields map holding the operation data.
    pub(crate) fields: Option<OperationFields>,
}

/// Create new operations.
///
/// Creating operations with the `OperationBuilder` does not validate them yet against their
/// claimed schemas. You can use `validate_operation` for this.
#[derive(Clone, Debug)]
pub struct OperationBuilder {
    /// Action of this operation.
    action: OperationAction,

    /// Schema instance of this operation.
    schema_id: SchemaId,

    /// Previous field which contains the last known view id for the target document.
    previous: Option<DocumentViewId>,

    /// Operation fields.
    fields: Option<OperationFields>,
}

impl OperationBuilder {
    /// Returns a new instance of `OperationBuilder`.
    pub fn new(schema_id: &SchemaId) -> Self {
        Self {
            action: OperationAction::Create,
            schema_id: schema_id.to_owned(),
            previous: None,
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

    /// Set previous operations.
    pub fn previous(mut self, previous: &DocumentViewId) -> Self {
        self.previous = Some(previous.to_owned());
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

    /// Builds and returns a new `Operation` instance.
    ///
    /// This method checks if the given previous operations and operation fields are matching the
    /// regarding operation action.
    pub fn build(&self) -> Result<Operation, OperationBuilderError> {
        let operation = Operation {
            action: self.action,
            version: OperationVersion::V1,
            schema_id: self.schema_id.to_owned(),
            previous: self.previous.to_owned(),
            fields: self.fields.to_owned(),
        };

        validate_operation_format(&operation)?;

        Ok(operation)
    }
}

impl AsOperation for Operation {
    /// Returns version of operation.
    fn version(&self) -> OperationVersion {
        self.version
    }

    /// Returns action type of operation.
    fn action(&self) -> OperationAction {
        self.action
    }

    /// Returns schema id of operation.
    fn schema_id(&self) -> SchemaId {
        self.schema_id
    }

    /// Returns known previous operations vector of this operation.
    fn previous(&self) -> Option<DocumentViewId> {
        self.previous.clone()
    }

    /// Returns application data fields of operation.
    fn fields(&self) -> Option<OperationFields> {
        self.fields
    }
}

impl Actionable for Operation {
    fn version(&self) -> OperationVersion {
        self.version
    }

    fn action(&self) -> OperationAction {
        self.action
    }

    fn previous(&self) -> Option<&DocumentViewId> {
        self.previous.as_ref()
    }
}

impl Schematic for Operation {
    fn schema_id(&self) -> &SchemaId {
        &self.schema_id
    }

    fn fields(&self) -> Option<PlainFields> {
        self.fields.as_ref().map(PlainFields::from)
    }
}

/*#[cfg(test)]
mod tests {
    use rstest::rstest;

    use crate::document::DocumentViewId;
    use crate::operation::traits::AsOperation;
    use crate::operation::{OperationAction, OperationFields, OperationValue, OperationVersion};
    use crate::schema::SchemaId;
    use crate::test_utils::fixtures::{document_view_id, schema_id};

    use super::OperationBuilder;

    #[rstest]
    fn operation_builder(schema_id: SchemaId, document_view_id: DocumentViewId) {
        let fields = vec![
            ("firstname", "Peter".into()),
            ("lastname", "Panda".into()),
            ("year", 2020.into()),
        ];

        let operation = OperationBuilder::new(&schema_id)
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
        assert!(OperationBuilder::new(&schema_id)
            .fields(&[("year", 2020.into())])
            .build()
            .is_ok());

        // CREATE operations must not contain previous
        assert!(OperationBuilder::new(&schema_id)
            .action(OperationAction::Create)
            .fields(&[("year", 2020.into())])
            .previous(&document_view_id)
            .build()
            .is_err());

        // CREATE operations must contain fields
        assert!(OperationBuilder::new(&schema_id)
            .action(OperationAction::Create)
            .build()
            .is_err());

        // correct UPDATE operation
        assert!(OperationBuilder::new(&schema_id)
            .action(OperationAction::Update)
            .fields(&[("year", 2020.into())])
            .previous(&document_view_id)
            .build()
            .is_ok());

        // UPDATE operations must have fields
        assert!(OperationBuilder::new(&schema_id)
            .action(OperationAction::Update)
            .previous(&document_view_id)
            .build()
            .is_err());

        // UPDATE operations must have previous
        assert!(OperationBuilder::new(&schema_id)
            .action(OperationAction::Update)
            .fields(&[("year", 2020.into())])
            .build()
            .is_err());

        // correct DELETE operation
        assert!(OperationBuilder::new(&schema_id)
            .action(OperationAction::Delete)
            .previous(&document_view_id)
            .build()
            .is_ok());

        // DELETE operations must not have fields
        assert!(OperationBuilder::new(&schema_id)
            .action(OperationAction::Delete)
            .previous(&document_view_id)
            .fields(&[("year", 2020.into())])
            .build()
            .is_err());

        // DELETE operations must have previous
        assert!(OperationBuilder::new(&schema_id)
            .action(OperationAction::Update)
            .build()
            .is_err());
    }

    #[rstest]
    fn field_ordering(schema_id: SchemaId) {
        // Create first test operation
        let operation_1 = OperationBuilder::new(&schema_id)
            .fields(&[("a", "sloth".into()), ("b", "penguin".into())])
            .build();

        // Create second test operation with same values but different order of fields
        let operation_2 = OperationBuilder::new(&schema_id)
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
}*/
