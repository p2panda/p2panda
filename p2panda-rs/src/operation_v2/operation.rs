// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::document::DocumentId;
use crate::document::DocumentViewId;
use crate::hash_v2::Hash;
use crate::hash_v2::HashId;
use crate::identity_v2::KeyPair;
use crate::operation_v2::body::encode::encode_body;
use crate::operation_v2::body::plain::{PlainFields, PlainOperation};
use crate::operation_v2::body::traits::Schematic;
use crate::operation_v2::body::Body;
use crate::operation_v2::error::OperationBuilderError;
use crate::operation_v2::header::encode::{encode_header, sign_header};
use crate::operation_v2::header::traits::{Actionable, Authored};
use crate::operation_v2::header::{Header, HeaderAction, HeaderExtension};
use crate::operation_v2::traits::AsOperation;
use crate::operation_v2::{
    OperationAction, OperationFields, OperationId, OperationValue, OperationVersion,
};
use crate::schema::SchemaId;
use crate::Validate;

use super::error::ValidateOperationError;

#[derive(Clone, Debug, PartialEq)]
pub struct Operation(OperationId, Header, Body);

impl Operation {
    pub(crate) fn new(
        operation_id: OperationId,
        header: Header,
        body: Body,
    ) -> Result<Self, ValidateOperationError> {
        let operation = Self(operation_id, header, body);
        operation.validate()?;
        Ok(operation)
    }

    pub fn header(&self) -> &Header {
        &self.1
    }

    pub fn body(&self) -> &Body {
        &self.2
    }
}

impl Validate for Operation {
    type Error = ValidateOperationError;

    fn validate(&self) -> Result<(), Self::Error> {
        let HeaderExtension {
            document_id,
            previous,
            timestamp,
            backlink,
            ..
        } = &self.header().4;

        let document_id = match document_id {
            Some(document_id) => document_id.to_owned(),
            None => return Err(ValidateOperationError::ExpectedDocumentId),
        };

        if timestamp.is_none() {
            return Err(ValidateOperationError::ExpectedTimestamp);
        }

        match self.action() {
            OperationAction::Create => {
                if self.id().as_hash() != document_id.as_hash() {
                    return Err(ValidateOperationError::IncorrectDocumentId(
                        document_id.to_string(),
                        self.id().to_string(),
                    ));
                };

                if backlink.is_some() {
                    return Err(ValidateOperationError::UnexpectedBacklink);
                }

                if self.fields().is_none() {
                    return Err(ValidateOperationError::ExpectedFields);
                }
                Ok(())
            }
            OperationAction::Update => {
                if backlink.is_none() {
                    return Err(ValidateOperationError::ExpectedBacklink);
                }

                if self.fields().is_none() {
                    return Err(ValidateOperationError::ExpectedFields);
                }
                Ok(())
            }
            OperationAction::Delete => {
                if backlink.is_none() {
                    return Err(ValidateOperationError::ExpectedBacklink);
                }

                if self.fields().is_some() {
                    return Err(ValidateOperationError::UnexpectedFields);
                }
                Ok(())
            }
        }
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

    /// Set document id.
    pub fn document_id(mut self, document_id: &DocumentId) -> Self {
        self.header_extension.document_id = Some(document_id.to_owned());
        self
    }

    /// Set timestamp.
    pub fn timestamp(mut self, timestamp: u64) -> Self {
        self.header_extension.timestamp = Some(timestamp.to_owned());
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
        let header_hash = encode_header(&header)?.hash();
        let operation = Operation::new(header_hash.into(), header, self.body)?;
        Ok(operation)
    }
}

impl AsOperation for Operation {
    /// Id of this operation.
    fn id(&self) -> &OperationId {
        &self.0
    }

    /// Timestamp
    fn timestamp(&self) -> u64 {
        // Safely unwrap as validation was performed already.
        self.header().4.timestamp.unwrap()
    }

    /// Hash of the preceding operation in an authors log, None if this is the first operation.
    fn backlink(&self) -> Option<&Hash> {
        self.header().4.backlink.as_ref()
    }

    /// Returns application data fields of operation.
    fn fields(&self) -> Option<&OperationFields> {
        self.body().1.as_ref()
    }
}

impl Actionable for Operation {
    fn version(&self) -> OperationVersion {
        self.header().0
    }

    fn action(&self) -> OperationAction {
        let HeaderExtension {
            action, previous, ..
        } = self.header().extension();
        match (action, previous) {
            (None, None) => OperationAction::Create,
            (None, Some(_)) => OperationAction::Update,
            (Some(HeaderAction::Delete), Some(_)) => OperationAction::Delete,
            // If correct validation was performed this case will not occur.
            (Some(HeaderAction::Delete), None) => unreachable!(),
        }
    }

    fn document_id(&self) -> &DocumentId {
        // Safely unwrap as we validated already that the operation header contains required field
        // document id.
        self.header().extension().document_id.as_ref().unwrap()
    }

    fn previous(&self) -> Option<&DocumentViewId> {
        self.header().extension().previous.as_ref()
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

    fn payload_hash(&self) -> &Hash {
        self.header().payload_hash()
    }

    fn signature(&self) -> crate::identity_v2::Signature {
        self.header().signature()
    }
}
