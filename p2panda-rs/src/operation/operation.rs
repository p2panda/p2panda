// SPDX-License-Identifier: AGPL-3.0-or-later

use crate::document::{DocumentId, DocumentViewId};
use crate::hash::Hash;
use crate::identity::KeyPair;
use crate::operation::body::encode::encode_body;
use crate::operation::body::plain::PlainFields;
use crate::operation::body::traits::Schematic;
use crate::operation::body::Body;
use crate::operation::error::{OperationBuilderError, ValidateOperationError};
use crate::operation::header::encode::{encode_header, sign_header};
use crate::operation::header::traits::{Actionable, Authored};
use crate::operation::header::{Header, HeaderAction, HeaderExtension};
use crate::operation::traits::AsOperation;
use crate::operation::{
    OperationAction, OperationFields, OperationId, OperationValue, OperationVersion,
};
use crate::schema::SchemaId;
use crate::Validate;

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
            action,
            document_id,
            previous,
            timestamp,
            backlink,
            ..
        } = &self.header().4;

        // All operations require a timestamp
        if timestamp.is_none() {
            return Err(ValidateOperationError::ExpectedTimestamp);
        }

        let action = match (action, previous) {
            (None, None) => OperationAction::Create,
            (None, Some(_)) => OperationAction::Update,
            (Some(HeaderAction::Delete), Some(_)) => OperationAction::Delete,
            (Some(HeaderAction::Delete), None) => {
                return Err(ValidateOperationError::ExpectedPreviousOperations)
            }
        };

        match (action, self.has_fields()) {
            (OperationAction::Delete, true) => Err(ValidateOperationError::UnexpectedFields),
            (OperationAction::Create | OperationAction::Update, false) => {
                Err(ValidateOperationError::ExpectedFields)
            }
            _ => Ok(()),
        }?;

        match action {
            OperationAction::Create => {
                if document_id.is_some() {
                    return Err(ValidateOperationError::UnexpectedDocumentId);
                }

                if backlink.is_some() {
                    return Err(ValidateOperationError::UnexpectedBacklink);
                }

                if previous.is_some() {
                    return Err(ValidateOperationError::UnexpectedPreviousOperations);
                }
                Ok(())
            }
            OperationAction::Update | OperationAction::Delete => {
                if document_id.is_none() {
                    return Err(ValidateOperationError::ExpectedDocumentId);
                }

                if previous.is_none() {
                    return Err(ValidateOperationError::ExpectedPreviousOperations);
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
    pub fn new(schema_id: &SchemaId, timestamp: u64) -> Self {
        let mut header_extension = HeaderExtension::default();
        header_extension.timestamp = Some(timestamp);

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

    /// Set operation backlink.
    pub fn backlink(mut self, backlink: &Hash) -> Self {
        self.header_extension.backlink = Some(backlink.to_owned());
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

    fn document_id(&self) -> DocumentId {
        match self.header().extension().document_id.as_ref() {
            Some(document_id) => document_id.clone(),
            None => DocumentId::new(self.id()),
        }
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
    fn public_key(&self) -> &crate::identity::PublicKey {
        self.header().public_key()
    }

    fn payload_size(&self) -> u64 {
        self.header().payload_size()
    }

    fn payload_hash(&self) -> &Hash {
        self.header().payload_hash()
    }

    fn signature(&self) -> crate::identity::Signature {
        self.header().signature()
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use crate::document::{DocumentId, DocumentViewId};
    use crate::hash::Hash;
    use crate::identity::KeyPair;
    use crate::operation::body::traits::Schematic;
    use crate::operation::header::traits::Actionable;
    use crate::operation::header::HeaderAction;
    use crate::operation::traits::AsOperation;
    use crate::operation::{
        OperationAction, OperationBuilder, OperationFields, OperationValue, OperationVersion,
    };
    use crate::schema::SchemaId;
    use crate::test_utils::fixtures::{
        document_id, document_view_id, key_pair, random_hash, schema_id,
    };

    #[rstest]
    fn operation_builder_create(key_pair: KeyPair, schema_id: SchemaId) {
        let fields = vec![
            ("firstname", "Peter".into()),
            ("lastname", "Panda".into()),
            ("year", 2020.into()),
        ];

        let timestamp = 1703027623;

        let operation = OperationBuilder::new(&schema_id, timestamp)
            .fields(&fields)
            .sign(&key_pair)
            .unwrap();

        assert_eq!(operation.version(), OperationVersion::V1);
        assert_eq!(operation.action(), OperationAction::Create);
        assert_eq!(operation.schema_id(), &schema_id);
        assert_eq!(operation.document_id(), DocumentId::new(operation.id()));
        assert_eq!(operation.backlink(), None);
        assert_eq!(operation.previous(), None);
        assert_eq!(operation.timestamp(), timestamp);
        assert_eq!(operation.fields(), Some(&fields.into()));
    }

    #[rstest]
    fn operation_builder_update(
        key_pair: KeyPair,
        schema_id: SchemaId,
        #[from(random_hash)] backlink: Hash,
        document_id: DocumentId,
        document_view_id: DocumentViewId,
    ) {
        let fields = vec![
            ("firstname", "Peter".into()),
            ("lastname", "Panda".into()),
            ("year", 2020.into()),
        ];

        let timestamp = 1703027623;

        let operation = OperationBuilder::new(&schema_id, timestamp)
            .document_id(&document_id)
            .backlink(&backlink)
            .previous(&document_view_id)
            .fields(&fields)
            .sign(&key_pair)
            .unwrap();

        assert_eq!(operation.version(), OperationVersion::V1);
        assert_eq!(operation.action(), OperationAction::Update);
        assert_eq!(operation.schema_id(), &schema_id);
        assert_eq!(operation.document_id(), document_id);
        assert_eq!(operation.backlink(), Some(&backlink));
        assert_eq!(operation.previous(), Some(&document_view_id));
        assert_eq!(operation.timestamp(), timestamp);
        assert_eq!(operation.fields(), Some(&fields.into()));
    }

    #[rstest]
    fn operation_builder_delete(
        key_pair: KeyPair,
        schema_id: SchemaId,
        #[from(random_hash)] backlink: Hash,
        document_id: DocumentId,
        document_view_id: DocumentViewId,
    ) {
        let timestamp = 1703027623;

        let operation = OperationBuilder::new(&schema_id, timestamp)
            .action(HeaderAction::Delete)
            .document_id(&document_id)
            .backlink(&backlink)
            .previous(&document_view_id)
            .sign(&key_pair)
            .unwrap();

        assert_eq!(operation.version(), OperationVersion::V1);
        assert_eq!(operation.action(), OperationAction::Delete);
        assert_eq!(operation.schema_id(), &schema_id);
        assert_eq!(operation.document_id(), document_id);
        assert_eq!(operation.backlink(), Some(&backlink));
        assert_eq!(operation.previous(), Some(&document_view_id));
        assert_eq!(operation.timestamp(), timestamp);
        assert_eq!(operation.fields(), None);
    }

    #[rstest]
    fn operation_builder_validation(
        key_pair: KeyPair,
        schema_id: SchemaId,
        #[from(random_hash)] backlink: Hash,
        document_id: DocumentId,
        document_view_id: DocumentViewId,
    ) {
        let timestamp = 1703027623;

        // Correct CREATE operation
        assert!(OperationBuilder::new(&schema_id, timestamp)
            .fields(&[("year", 2020.into())])
            .sign(&key_pair)
            .is_ok());

        // CREATE operations must not contain previous
        assert!(OperationBuilder::new(&schema_id, timestamp)
            .previous(&document_view_id)
            .fields(&[("year", 2020.into())])
            .sign(&key_pair)
            .is_err());

        // CREATE operations must not contain backlink
        assert!(OperationBuilder::new(&schema_id, timestamp)
            .backlink(&backlink)
            .fields(&[("year", 2020.into())])
            .sign(&key_pair)
            .is_err());

        // CREATE operations must contain fields
        assert!(OperationBuilder::new(&schema_id, timestamp)
            .sign(&key_pair)
            .is_err());

        // correct UPDATE operation
        assert!(OperationBuilder::new(&schema_id, timestamp)
            .document_id(&document_id)
            .backlink(&backlink)
            .previous(&document_view_id)
            .fields(&[("year", 2020.into())])
            .sign(&key_pair)
            .is_ok());

        // UPDATE operations must have fields
        assert!(OperationBuilder::new(&schema_id, timestamp)
            .document_id(&document_id)
            .backlink(&backlink)
            .previous(&document_view_id)
            .sign(&key_pair)
            .is_err());

        // UPDATE operations must have previous
        assert!(OperationBuilder::new(&schema_id, timestamp)
            .document_id(&document_id)
            .backlink(&backlink)
            .fields(&[("year", 2020.into())])
            .sign(&key_pair)
            .is_err());

        // UPDATE operations must have document id
        assert!(OperationBuilder::new(&schema_id, timestamp)
            .backlink(&backlink)
            .previous(&document_view_id)
            .fields(&[("year", 2020.into())])
            .sign(&key_pair)
            .is_err());

        // correct DELETE operation
        assert!(OperationBuilder::new(&schema_id, timestamp)
            .action(HeaderAction::Delete)
            .document_id(&document_id)
            .backlink(&backlink)
            .previous(&document_view_id)
            .sign(&key_pair)
            .is_ok());

        // DELETE operations must not have fields
        assert!(OperationBuilder::new(&schema_id, timestamp)
            .action(HeaderAction::Delete)
            .document_id(&document_id)
            .backlink(&backlink)
            .previous(&document_view_id)
            .fields(&[("year", 2020.into())])
            .sign(&key_pair)
            .is_err());

        // DELETE operations must have previous
        assert!(OperationBuilder::new(&schema_id, timestamp)
            .action(HeaderAction::Delete)
            .document_id(&document_id)
            .backlink(&backlink)
            .sign(&key_pair)
            .is_err());

        // DELETE operations must have document id
        assert!(OperationBuilder::new(&schema_id, timestamp)
            .action(HeaderAction::Delete)
            .backlink(&backlink)
            .previous(&document_view_id)
            .sign(&key_pair)
            .is_err());

        // DELETE operation must have backlink
        assert!(OperationBuilder::new(&schema_id, timestamp)
            .action(HeaderAction::Delete)
            .document_id(&document_id)
            .previous(&document_view_id)
            .sign(&key_pair)
            .is_err());
    }

    #[rstest]
    fn field_ordering(key_pair: KeyPair, schema_id: SchemaId) {
        let timestamp = 1703027623;

        // Create first test operation
        let operation_1 = OperationBuilder::new(&schema_id, timestamp)
            .fields(&[("a", "sloth".into()), ("b", "penguin".into())])
            .sign(&key_pair);

        // Create second test operation with same values but different order of fields
        let operation_2 = OperationBuilder::new(&schema_id, timestamp)
            .fields(&[("b", "penguin".into()), ("a", "sloth".into())])
            .sign(&key_pair);

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
