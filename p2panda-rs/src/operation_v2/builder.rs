// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::document::DocumentViewId;
use crate::identity_v2::PrivateKey;
use crate::operation::Operation;
use crate::operation_v2::body::traits::{Actionable, AsOperation, Schematic};
use crate::operation_v2::body::validate::validate_operation_format;
use crate::operation_v2::body::{
    OperationAction, OperationFields, OperationValue, OperationVersion,
};
use crate::operation_v2::error::OperationBuilderError;
use crate::operation_v2::header::Header;
use crate::schema::SchemaId;

/// Create new operations.
///
/// Creating operations with the `OperationBuilder` does not validate them yet against their
/// claimed schemas. You can use `validate_operation` for this.
#[derive(Clone, Debug)]
pub struct OperationBuilder {
    /// Previous field which contains the last known view id for the target document.
    previous: Option<DocumentViewId>,

    seq_num: Option<u64>,

    timestamp: Option<u64>,

    /// Action of this operation.
    action: OperationAction,

    /// Schema instance of this operation.
    schema_id: SchemaId,

    /// Operation fields.
    fields: Option<OperationFields>,
}

impl OperationBuilder {
    /// Returns a new instance of `OperationBuilder`.
    pub fn new(schema_id: &SchemaId) -> Self {
        Self {
            // Header
            previous: None,
            seq_num: None,
            timestamp: None,

            // Body
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

    /// Set previous operations.
    pub fn previous(mut self, previous: &DocumentViewId) -> Self {
        self.previous = Some(previous.to_owned());
        self
    }

    pub fn seq_num(mut self, seq_num: u64) -> Self {
        self.seq_num = Some(seq_num);
        self
    }

    pub fn timestamp(mut self, timestamp: u64) -> Self {
        self.timestamp = Some(timestamp);
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
    pub fn sign(
        &self,
        private_key: &PrivateKey,
    ) -> Result<(Header, Operation), OperationBuilderError> {
        let header = Header();

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
