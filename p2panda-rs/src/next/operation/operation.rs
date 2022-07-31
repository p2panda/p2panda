// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::next::document::DocumentViewId;
use crate::next::operation::error::OperationBuilderError;
use crate::next::operation::plain::PlainFields;
use crate::next::operation::traits::{Actionable, AsOperation, Schematic};
use crate::next::operation::validate::validate_operation;
use crate::next::operation::{OperationAction, OperationFields, OperationValue, OperationVersion};
use crate::next::schema::{Schema, SchemaId};

#[derive(Debug)]
pub struct OperationBuilder {
    action: OperationAction,
    schema: Schema,
    previous_operations: Option<DocumentViewId>,
    fields: Option<OperationFields>,
}

impl OperationBuilder {
    pub fn new(schema: &Schema) -> Self {
        Self {
            action: OperationAction::Create,
            schema: schema.to_owned(),
            previous_operations: None,
            fields: None,
        }
    }

    pub fn action(mut self, action: &OperationAction) -> Self {
        self.action = action.to_owned();
        self
    }

    pub fn schema(mut self, schema: &Schema) -> Self {
        self.schema = schema.to_owned();
        self
    }

    pub fn previous_operations(mut self, previous_operations: &DocumentViewId) -> Self {
        self.previous_operations = Some(previous_operations.to_owned());
        self
    }

    pub fn fields(mut self, fields: &[(&str, OperationValue)]) -> Self {
        let mut operation_fields = OperationFields::new();

        for (field_name, field_value) in fields {
            operation_fields
                .insert(field_name, field_value.to_owned())
                // @TODO: Ignore error, we avoid duplicates with `insert`
                .unwrap();
        }

        self.fields = Some(operation_fields);
        self
    }

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
    use std::collections::HashMap;
    use std::convert::TryFrom;

    use rstest::rstest;
    use rstest_reuse::apply;

    use crate::next::document::{DocumentId, DocumentViewId};
    use crate::next::operation::traits::AsOperation;
    use crate::next::operation::{EncodedOperation, OperationValue, Relation};
    use crate::next::schema::SchemaId;
    use crate::next::test_utils::fixtures::{
        operation_fields, random_document_id, random_document_view_id, schema_id,
    };
    use crate::next::test_utils::templates::many_valid_operations;
    use crate::Validate;

    use super::{Operation, OperationAction, OperationFields, OperationVersion};

    #[test]
    fn stringify_action() {
        assert_eq!(OperationAction::Create.as_str(), "create");
        assert_eq!(OperationAction::Update.as_str(), "update");
        assert_eq!(OperationAction::Delete.as_str(), "delete");
    }

    // @TODO: Move this to `operation::validate`
    /* #[rstest]
    fn operation_validation(
        operation_fields: OperationFields,
        schema: SchemaId,
        #[from(random_document_view_id)] prev_op_id: DocumentViewId,
    ) {
        let invalid_create_operation_1 = Operation {
            action: OperationAction::Create,
            version: OperationVersion::Default,
            schema: schema.clone(),
            previous_operations: None,
            // CREATE operations must contain fields
            fields: None, // Error
        };

        assert!(invalid_create_operation_1.validate().is_err());

        let invalid_create_operation_2 = Operation {
            action: OperationAction::Create,
            version: OperationVersion::Default,
            schema: schema.clone(),
            // CREATE operations must not contain previous_operations
            previous_operations: Some(prev_op_id.clone()), // Error
            fields: Some(operation_fields.clone()),
        };

        assert!(invalid_create_operation_2.validate().is_err());

        let invalid_update_operation_1 = Operation {
            action: OperationAction::Update,
            version: OperationVersion::Default,
            schema: schema.clone(),
            // UPDATE operations must contain previous_operations
            previous_operations: None, // Error
            fields: Some(operation_fields.clone()),
        };

        assert!(invalid_update_operation_1.validate().is_err());

        let invalid_update_operation_2 = Operation {
            action: OperationAction::Update,
            version: OperationVersion::Default,
            schema: schema.clone(),
            previous_operations: Some(prev_op_id.clone()),
            // UPDATE operations must contain fields
            fields: None, // Error
        };

        assert!(invalid_update_operation_2.validate().is_err());

        let invalid_delete_operation_1 = Operation {
            action: OperationAction::Delete,
            version: OperationVersion::Default,
            schema: schema.clone(),
            // DELETE operations must contain previous_operations
            previous_operations: None, // Error
            fields: None,
        };

        assert!(invalid_delete_operation_1.validate().is_err());

        let invalid_delete_operation_2 = Operation {
            action: OperationAction::Delete,
            version: OperationVersion::Default,
            schema,
            previous_operations: Some(prev_op_id),
            // DELETE operations must not contain fields
            fields: Some(operation_fields), // Error
        };

        assert!(invalid_delete_operation_2.validate().is_err());
    } */

    // @TODO: Move this to `operation::encode` or `decode` or `tests`
    /* #[rstest]
    fn encode_and_decode(
        schema: SchemaId,
        #[from(random_document_view_id)] prev_op_id: DocumentViewId,
        #[from(random_document_id)] document_id: DocumentId,
    ) {
        // Create test operation
        let mut fields = OperationFields::new();

        // Add one field for every kind of OperationValue
        fields
            .add("username", OperationValue::String("bubu".to_owned()))
            .unwrap();

        fields.add("height", OperationValue::Float(3.5)).unwrap();

        fields.add("age", OperationValue::Integer(28)).unwrap();

        fields
            .add("is_admin", OperationValue::Boolean(false))
            .unwrap();

        fields
            .add(
                "profile_picture",
                OperationValue::Relation(Relation::new(document_id)),
            )
            .unwrap();

        let operation = Operation::new_update(schema, prev_op_id, fields).unwrap();

        assert!(operation.is_update());

        // Encode operation ...
        let encoded = EncodedOperation::try_from(&operation).unwrap();

        // ... and decode it again
        let operation_restored = Operation::try_from(&encoded).unwrap();

        assert_eq!(operation, operation_restored);
    } */

    // @TODO: This can stay here, just needs refactoring
    /* #[rstest]
    fn field_ordering(schema_id: SchemaId) {
        // Create first test operation
        let mut fields = OperationFields::new();
        fields
            .add("a", OperationValue::String("sloth".to_owned()))
            .unwrap();
        fields
            .add("b", OperationValue::String("penguin".to_owned()))
            .unwrap();

        let first_operation = Operation::new_create(schema.clone(), fields).unwrap();

        // Create second test operation with same values but different order of fields
        let mut second_fields = OperationFields::new();
        second_fields
            .add("b", OperationValue::String("penguin".to_owned()))
            .unwrap();
        second_fields
            .add("a", OperationValue::String("sloth".to_owned()))
            .unwrap();

        let second_operation = Operation::new_create(schema, second_fields).unwrap();

        assert_eq!(first_operation.to_cbor(), second_operation.to_cbor());
    } */

    #[test]
    fn field_iteration() {
        // Create first test operation
        let mut fields = OperationFields::new();
        fields
            .add("a", OperationValue::String("sloth".to_owned()))
            .unwrap();
        fields
            .add("b", OperationValue::String("penguin".to_owned()))
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

    // @TODO: Move this to `operation::validate`
    /* #[apply(many_valid_operations)]
    fn many_valid_operations_should_encode(#[case] operation: Operation) {
        assert!(EncodedOperation::try_from(&operation).is_ok())
    } */

    // @TODO: Do we still need this?
    /* fn it_hashes(operation: Operation) {
        let mut hash_map = HashMap::new();
        let key_value = "Value identified by a hash".to_string();
        hash_map.insert(&operation, key_value.clone());
        let key_value_retrieved = hash_map.get(&operation).unwrap().to_owned();
        assert_eq!(key_value, key_value_retrieved)
    } */
}
