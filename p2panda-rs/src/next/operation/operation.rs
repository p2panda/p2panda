// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::next::document::DocumentViewId;
use crate::next::operation::error::ValidateOperationError;
use crate::next::operation::plain::PlainFields;
use crate::next::operation::traits::{Actionable, AsOperation, Schematic};
use crate::next::operation::validate::validate_operation;
use crate::next::operation::{OperationAction, OperationFields, OperationValue, OperationVersion};
use crate::next::schema::{Schema, SchemaId};

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

    pub fn build(&self) -> Result<Operation, ValidateOperationError> {
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
// @TODO: Fix pub(crate) visibility
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
        if let Some(inner) = &self.fields {
            Some(PlainFields::from(inner))
        } else {
            None
        }
    }
}
