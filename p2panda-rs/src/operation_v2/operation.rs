// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::document::DocumentViewId;
use crate::identity_v2::KeyPair;
use crate::operation_v2::body::encode::encode_body;
use crate::operation_v2::body::plain::PlainFields;
use crate::operation_v2::body::plain::PlainOperation;
use crate::operation_v2::body::traits::Schematic;
use crate::operation_v2::body::Body;
use crate::operation_v2::error::OperationBuilderError;
use crate::operation_v2::header::encode::sign_header;
use crate::operation_v2::header::traits::{Actionable, Authored};
use crate::operation_v2::header::{Header, HeaderAction, HeaderExtension};
use crate::operation_v2::traits::AsOperation;
use crate::operation_v2::validate::validate_operation_format;
use crate::operation_v2::{OperationAction, OperationFields, OperationValue, OperationVersion};
use crate::schema::SchemaId;

#[derive(Clone, Debug, PartialEq)]
pub struct Operation(Header, Body);

impl Operation {
    pub(crate) fn new(header: Header, body: Body) -> Self {
        Self(header, body)
    }

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
        let plain_operation: PlainOperation = (&self.body).into();
        let header = sign_header(self.header_extension, &payload, key_pair)?;
        validate_operation_format(&header, &plain_operation)?;
        let operation = Operation::new(header, self.body);
        Ok(operation)
    }
}

impl AsOperation for Operation {
    /// Returns application data fields of operation.
    fn fields(&self) -> Option<&OperationFields> {
        self.body().1.as_ref()
    }
}

impl Actionable for Operation {
    fn version(&self) -> OperationVersion {
        self.header().version()
    }

    fn action(&self) -> OperationAction {
        self.header().action()
    }

    fn previous(&self) -> Option<&DocumentViewId> {
        self.header().previous()
    }
}

impl Schematic for Operation {
    fn schema_id(&self) -> &SchemaId {
        &self.body().schema_id()
    }

    fn plain_fields(&self) -> Option<PlainFields> {
        self.body().plain_fields()
    }
}

impl Authored for Operation {
    fn public_key(&self) -> &crate::identity_v2::PublicKey {
        self.header().public_key()
    }

    fn payload_size(&self) -> u64 {
        self.header().payload_size()
    }

    fn payload_hash(&self) -> &crate::hash_v2::Hash {
        self.header().payload_hash()
    }

    fn signature(&self) -> crate::identity_v2::Signature {
        self.header().signature()
    }
}
