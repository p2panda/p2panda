// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::document::DocumentViewId;
use crate::identity_v2::KeyPair;
use crate::operation_v2::body::encode::encode_body;
use crate::operation_v2::body::plain::PlainFields;
use crate::operation_v2::body::Body;
use crate::operation_v2::error::OperationBuilderError;
use crate::operation_v2::header::encode::sign_header;
use crate::operation_v2::header::traits::AsHeader;
use crate::operation_v2::header::{Header, HeaderAction, HeaderExtension};
use crate::operation_v2::traits::{Actionable, AsOperation, Schematic};
use crate::operation_v2::validate::validate_operation_format;
use crate::operation_v2::{OperationAction, OperationFields, OperationValue, OperationVersion};
use crate::schema::SchemaId;

#[derive(Clone, Debug, PartialEq)]
pub struct Operation(pub(crate) Header, pub(crate) Body);

impl Operation {
    pub fn header(&self) -> &Header {
        &self.0
    }

    pub fn body(&self) -> &Body {
        &self.1
    }
}

#[derive(Clone, Debug)]
pub struct OperationBuilder {
    header_extension: HeaderExtension,
    body: Body,
}

impl OperationBuilder {
    /// Returns a new instance of `OperationBuilder`.
    pub fn new(schema_id: &SchemaId) -> Self {
        let header_extension = HeaderExtension::default();
        let body = Body(schema_id.to_owned(), None);

        Self {
            header_extension,
            body,
        }
    }

    /// Set operation action.
    pub fn action(mut self, action: HeaderAction) -> Self {
        self.header_extension.action = Some(action);
        self
    }

    /// Set operation schema.
    pub fn schema_id(mut self, schema_id: SchemaId) -> Self {
        self.body.0 = schema_id;
        self
    }

    /// Set previous operations.
    pub fn previous(mut self, previous: &DocumentViewId) -> Self {
        self.header_extension.previous = Some(previous.to_owned());
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

        self.body.1 = Some(operation_fields);
        self
    }

    /// Builds and returns a new `Operation` instance.
    ///
    /// This method checks if the given previous operations and operation fields are matching the
    /// regarding operation action.
    pub fn sign(self, key_pair: &KeyPair) -> Result<Operation, OperationBuilderError> {
        let payload = encode_body(&self.body)?;
        let header = sign_header(self.header_extension, &payload, key_pair)?;
        let operation = Operation(header, self.body);
        validate_operation_format(&operation)?;
        Ok(operation)
    }
}

impl AsOperation for Operation {
    /// Returns version of operation.
    fn version(&self) -> OperationVersion {
        AsHeader::version(self.header())
    }

    /// Returns action type of operation.
    fn action(&self) -> OperationAction {
        self.header().action()
    }

    /// Returns schema id of operation.
    fn schema_id(&self) -> SchemaId {
        self.body().schema_id().to_owned()
    }

    /// Returns known previous operations vector of this operation.
    fn previous(&self) -> Option<DocumentViewId> {
        self.header().extensions().previous
    }

    /// Returns application data fields of operation.
    fn fields(&self) -> Option<OperationFields> {
        todo!()
    }
}

impl Actionable for Operation {
    fn version(&self) -> OperationVersion {
        AsHeader::version(self.header())
    }

    fn action(&self) -> OperationAction {
        self.header().action()
    }

    fn previous(&self) -> Option<&DocumentViewId> {
        self.header().extensions().previous.as_ref()
    }
}

impl Schematic for Operation {
    fn schema_id(&self) -> &SchemaId {
        &self.body().schema_id()
    }

    fn fields(&self) -> Option<PlainFields> {
        (&self.body().fields()).map(PlainFields::from)
    }
}
