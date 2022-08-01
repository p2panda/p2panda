// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::next::document::DocumentViewId;
use crate::next::operation::error::OperationBuilderError;
use crate::next::operation::plain::PlainFields;
use crate::next::operation::traits::{Actionable, AsOperation, Schematic};
use crate::next::operation::validate::validate_operation;
use crate::next::operation::{OperationAction, OperationFields, OperationValue, OperationVersion};
use crate::next::schema::{Schema, SchemaId};

/// Create new operations.
#[derive(Clone, Debug)]
pub struct OperationBuilder {
    /// Action of this operation.
    action: OperationAction,

    /// Schema instance of this operation.
    schema: Schema,

    /// Previous operations field.
    previous_operations: Option<DocumentViewId>,

    /// Operation fields.
    fields: Option<OperationFields>,
}

impl OperationBuilder {
    /// Returns a new instance of `OperationBuilder`.
    pub fn new(schema: &Schema) -> Self {
        Self {
            action: OperationAction::Create,
            schema: schema.to_owned(),
            previous_operations: None,
            fields: None,
        }
    }

    /// Set operation action.
    pub fn action(mut self, action: OperationAction) -> Self {
        self.action = action;
        self
    }

    /// Set operation schema.
    pub fn schema(mut self, schema: &Schema) -> Self {
        self.schema = schema.to_owned();
        self
    }

    /// Set previous operations.
    pub fn previous_operations(mut self, previous_operations: &DocumentViewId) -> Self {
        self.previous_operations = Some(previous_operations.to_owned());
        self
    }

    /// Set operation fields.
    pub fn fields(mut self, fields: &[(&str, OperationValue)]) -> Self {
        let mut operation_fields = OperationFields::new();

        for (field_name, field_value) in fields {
            if operation_fields
                .insert(field_name, field_value.to_owned())
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
            schema: self.schema.to_owned(),
            previous_operations: self.previous_operations.to_owned(),
            fields: self.fields.to_owned(),
        };

        validate_operation(&operation, &self.schema)?;

        Ok(operation)
    }
}

/// Operations describe data mutations of "documents" in the p2panda network. Authors send
/// operations to CREATE, UPDATE or DELETE documents.
///
/// The data itself lives in the "fields" object and is formed after an operation schema.
///
/// Starting from an initial CREATE operation, the following collection of UPDATE operations build
/// up a causal graph of mutations which can be resolved into a single object during a
/// "materialisation" process. If a DELETE operation is published it signals the deletion of the
/// entire graph and no more UPDATE operations should be published.
///
/// All UPDATE and DELETE operations have a `previous_operations` field which contains a vector of
/// operation ids which identify the known branch tips at the time of publication. These allow us
/// to build the graph and retain knowledge of the graph state at the time the specific operation
/// was published.
#[derive(Clone, Debug, PartialEq)]
pub struct Operation {
    /// Version of this operation.
    pub(crate) version: OperationVersion,

    /// Describes if this operation creates, updates or deletes data.
    pub(crate) action: OperationAction,

    /// Schema matching this operation.
    pub(crate) schema: Schema,

    /// Optional document view id containing the operation ids directly preceding this one in the
    /// document.
    pub(crate) previous_operations: Option<DocumentViewId>,

    /// Optional fields map holding the operation data.
    pub(crate) fields: Option<OperationFields>,
}

impl AsOperation for Operation {
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
        self.schema.id().to_owned()
    }

    /// Returns known previous operations vector of this operation.
    fn previous_operations(&self) -> Option<DocumentViewId> {
        self.previous_operations.clone()
    }

    /// Returns application data fields of operation.
    fn fields(&self) -> Option<OperationFields> {
        self.fields.clone()
    }
}

impl Actionable for Operation {
    fn version(&self) -> OperationVersion {
        self.version
    }

    fn action(&self) -> OperationAction {
        self.action
    }

    fn previous_operations(&self) -> Option<&DocumentViewId> {
        self.previous_operations.as_ref()
    }
}

impl Schematic for Operation {
    fn schema_id(&self) -> &SchemaId {
        self.schema.id()
    }

    fn fields(&self) -> Option<PlainFields> {
        self.fields.as_ref().map(PlainFields::from)
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use crate::next::document::{DocumentId, DocumentViewId};
    use crate::next::operation::traits::AsOperation;
    use crate::next::operation::{
        OperationAction, OperationFields, OperationValue, OperationVersion,
    };
    use crate::next::schema::{FieldType, Schema};
    use crate::next::test_utils::constants::SCHEMA_ID;
    use crate::next::test_utils::fixtures::{document_id, document_view_id, schema, schema_id};

    use super::OperationBuilder;

    #[rstest]
    fn operation_builder(
        #[with(vec![
           ("firstname".into(), FieldType::String),
           ("lastname".into(), FieldType::String),
           ("year".into(), FieldType::Integer),
        ])]
        schema: Schema,
        document_view_id: DocumentViewId,
    ) {
        let fields = vec![
            ("firstname", "Peter".into()),
            ("lastname", "Panda".into()),
            ("year", 2020.into()),
        ];

        let operation = OperationBuilder::new(&schema)
            .action(OperationAction::Update)
            .previous_operations(&document_view_id)
            .fields(&fields)
            .build()
            .unwrap();

        assert_eq!(operation.action(), OperationAction::Update);
        assert_eq!(operation.previous_operations(), Some(document_view_id));
        assert_eq!(operation.fields(), Some(fields.into()));
        assert_eq!(operation.version(), OperationVersion::V1);
        assert_eq!(operation.schema_id(), *schema.id());
    }

    #[rstest]
    fn operation_builder_validation(
        #[with(vec![("year".into(), FieldType::Integer)])] schema: Schema,
        document_view_id: DocumentViewId,
    ) {
        // Correct CREATE operation
        assert!(OperationBuilder::new(&schema)
            .fields(&[("year", 2020.into())])
            .build()
            .is_ok());

        // CREATE operations should not contain previous_operations
        assert!(OperationBuilder::new(&schema)
            .action(OperationAction::Create)
            .previous_operations(&document_view_id)
            .fields(&[("year", 2020.into())])
            .build()
            .is_err());

        // UPDATE operations should have fields
        assert!(OperationBuilder::new(&schema)
            .action(OperationAction::Update)
            .previous_operations(&document_view_id)
            .build()
            .is_err());

        // DELETE operations should not have fields
        assert!(OperationBuilder::new(&schema)
            .action(OperationAction::Delete)
            .previous_operations(&document_view_id)
            .fields(&[("year", 2020.into())])
            .build()
            .is_err());
    }

    #[rstest]
    fn operation_builder_schema_validation(
        #[with(vec![
            ("firstname".into(), FieldType::String),
            ("year".into(), FieldType::Integer),
            ("is_cute".into(), FieldType::Boolean),
            ("address".into(), FieldType::Relation(schema_id(SCHEMA_ID))),
        ])]
        schema: Schema,
        document_id: DocumentId,
        document_view_id: DocumentViewId,
    ) {
        // Operation matches schema
        assert!(OperationBuilder::new(&schema)
            .fields(&[
                ("firstname", "Peter".into()),
                ("year", 2020.into()),
                ("is_cute", false.into()),
                ("address", document_id.clone().into()),
            ])
            .build()
            .is_ok());

        // Field ordering does not matter in builder
        assert!(OperationBuilder::new(&schema)
            .fields(&[
                ("address", document_id.clone().into()),
                ("is_cute", false.into()),
                ("year", 2020.into()),
                ("firstname", "Peter".into()),
            ])
            .build()
            .is_ok());

        // Field missing
        assert!(OperationBuilder::new(&schema)
            .fields(&[
                ("firstname", "Peter".into()),
                ("is_cute", false.into()),
                ("address", document_id.clone().into()),
            ])
            .build()
            .is_err());

        // Invalid type
        assert!(OperationBuilder::new(&schema)
            .fields(&[
                ("firstname", "Peter".into()),
                ("year", "2020".into()),
                ("is_cute", false.into()),
                ("address", document_id.clone().into()),
            ])
            .build()
            .is_err());

        // Correct UPDATE operation matching schema
        assert!(OperationBuilder::new(&schema)
            .action(OperationAction::Update)
            .previous_operations(&document_view_id)
            .fields(&[("address", document_id.into())])
            .build()
            .is_ok());
    }

    #[rstest]
    fn field_ordering(
        #[with(vec![
           ("a".into(), FieldType::String),
           ("b".into(), FieldType::String),
        ])]
        schema: Schema,
    ) {
        // Create first test operation
        let operation_1 = OperationBuilder::new(&schema)
            .fields(&[("a", "sloth".into()), ("b", "penguin".into())])
            .build();

        // Create second test operation with same values but different order of fields
        let operation_2 = OperationBuilder::new(&schema)
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
