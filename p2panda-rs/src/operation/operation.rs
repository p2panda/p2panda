// SPDX-License-Identifier: AGPL-3.0-or-later

use std::time::{SystemTime, UNIX_EPOCH};

use crate::document::{self, DocumentId, DocumentViewId};
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

#[derive(Clone, Debug)]
pub struct OperationBuilder {
    header_extension: HeaderExtension,
    body: Body,
}

impl Validate for Operation {
    type Error = ValidateOperationError;

    fn validate(&self) -> Result<(), Self::Error> {
        // We validate only the strictest requirements expected of an operation here. To see all validation
        // which is required of operations by current reference implementations of p2panda please
        // look into the `api/validation` module.

        // What is validated here:
        // - check the header follows minimum requirements (see Header::Validate)
        // - CREATE and UPDATE operations must contain fields
        // - DELETE operations must not contain fields
        self.header().validate()?;
        match (self.action(), self.fields()) {
            (OperationAction::Create | OperationAction::Update, None) => {
                Err(ValidateOperationError::ExpectedFields)
            }
            (OperationAction::Delete, Some(_)) => Err(ValidateOperationError::UnexpectedFields),
            (_, _) => Ok(()),
        }
    }
}

impl OperationBuilder {
    /// Returns a new instance of `OperationBuilder`.
    pub fn new(schema_id: &SchemaId) -> Self {
        let mut header_extension = HeaderExtension::default();
        // safely unwrap as we expect all times to be greater than "1970-01-01 00:00:00 UTC"
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        header_extension.timestamp = Some(timestamp);
        header_extension.depth = Some(0);

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

    /// Set unix timestamp in nanoseconds.
    pub fn timestamp(mut self, timestamp: u128) -> Self {
        self.header_extension.timestamp = Some(timestamp);
        self
    }

    /// Set document id.
    pub fn document_id(mut self, document_id: &DocumentId) -> Self {
        self.header_extension.document_id = Some(document_id.to_owned());
        self
    }

    /// Set depth.
    pub fn depth(mut self, depth: u64) -> Self {
        self.header_extension.depth = Some(depth);
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
    fn timestamp(&self) -> u128 {
        // Safely unwrap as validation was performed already.
        self.header().4.timestamp.unwrap()
    }

    /// Hash of the preceding operation in an authors log, None if this is the first operation.
    fn backlink(&self) -> Option<&Hash> {
        self.header().4.backlink.as_ref()
    }

    /// The distance (via the longest path) from this operation to the root of the operation graph.
    fn depth(&self) -> u64 {
        // Safely unwrap as validation performed already.
        self.header().4.depth.unwrap()
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
        match (self.header().extension().action, self.depth()) {
            (None, 0) => OperationAction::Create,
            (None, _) => OperationAction::Update,
            (Some(HeaderAction::Delete), _) => OperationAction::Delete,
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

        let operation = OperationBuilder::new(&schema_id)
            .fields(&fields)
            .sign(&key_pair)
            .unwrap();

        assert_eq!(operation.version(), OperationVersion::V1);
        assert_eq!(operation.action(), OperationAction::Create);
        assert_eq!(operation.schema_id(), &schema_id);
        assert_eq!(operation.document_id(), DocumentId::new(operation.id()));
        assert_eq!(operation.backlink(), None);
        assert_eq!(operation.previous(), None);
        assert_eq!(operation.depth(), 0);
        assert!(operation.header().extension().timestamp.is_some());
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

        let operation = OperationBuilder::new(&schema_id)
            .document_id(&document_id)
            .backlink(&backlink)
            .previous(&document_view_id)
            .depth(1)
            .fields(&fields)
            .sign(&key_pair)
            .unwrap();

        assert_eq!(operation.version(), OperationVersion::V1);
        assert_eq!(operation.action(), OperationAction::Update);
        assert_eq!(operation.schema_id(), &schema_id);
        assert_eq!(operation.document_id(), document_id);
        assert_eq!(operation.backlink(), Some(&backlink));
        assert_eq!(operation.previous(), Some(&document_view_id));
        assert_eq!(operation.depth(), 1);
        assert!(operation.header().extension().timestamp.is_some());
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
        let operation = OperationBuilder::new(&schema_id)
            .action(HeaderAction::Delete)
            .document_id(&document_id)
            .backlink(&backlink)
            .previous(&document_view_id)
            .depth(1)
            .sign(&key_pair)
            .unwrap();

        assert_eq!(operation.version(), OperationVersion::V1);
        assert_eq!(operation.action(), OperationAction::Delete);
        assert_eq!(operation.schema_id(), &schema_id);
        assert_eq!(operation.document_id(), document_id);
        assert_eq!(operation.backlink(), Some(&backlink));
        assert_eq!(operation.previous(), Some(&document_view_id));
        assert_eq!(operation.depth(), 1);
        assert!(operation.header().extension().timestamp.is_some());
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
        // Correct CREATE operation
        assert!(OperationBuilder::new(&schema_id)
            .fields(&[("year", 2020.into())])
            .sign(&key_pair)
            .is_ok());

        // CREATE operations must not contain previous
        assert!(OperationBuilder::new(&schema_id)
            .previous(&document_view_id)
            .fields(&[("year", 2020.into())])
            .sign(&key_pair)
            .is_err());

        // CREATE operations must not contain backlink
        assert!(OperationBuilder::new(&schema_id)
            .backlink(&backlink)
            .fields(&[("year", 2020.into())])
            .sign(&key_pair)
            .is_err());

        // CREATE operations must not contain non-zero depth
        assert!(OperationBuilder::new(&schema_id)
            .depth(1)
            .fields(&[("year", 2020.into())])
            .sign(&key_pair)
            .is_err());

        // CREATE operations must contain fields
        assert!(OperationBuilder::new(&schema_id).sign(&key_pair).is_err());

        // correct UPDATE operation
        assert!(OperationBuilder::new(&schema_id)
            .document_id(&document_id)
            .backlink(&backlink)
            .previous(&document_view_id)
            .depth(1)
            .fields(&[("year", 2020.into())])
            .sign(&key_pair)
            .is_ok());

        // UPDATE operation mut have non-zero depth
        assert!(OperationBuilder::new(&schema_id)
            .document_id(&document_id)
            .backlink(&backlink)
            .previous(&document_view_id)
            .depth(0)
            .fields(&[("year", 2020.into())])
            .sign(&key_pair)
            .is_err());

        // UPDATE operations must have fields
        assert!(OperationBuilder::new(&schema_id)
            .document_id(&document_id)
            .backlink(&backlink)
            .previous(&document_view_id)
            .depth(1)
            .sign(&key_pair)
            .is_err());

        // UPDATE operations must have previous
        assert!(OperationBuilder::new(&schema_id)
            .document_id(&document_id)
            .backlink(&backlink)
            .depth(1)
            .fields(&[("year", 2020.into())])
            .sign(&key_pair)
            .is_err());

        // UPDATE operations must have document id
        assert!(OperationBuilder::new(&schema_id)
            .backlink(&backlink)
            .previous(&document_view_id)
            .depth(1)
            .fields(&[("year", 2020.into())])
            .sign(&key_pair)
            .is_err());

        // correct DELETE operation
        assert!(OperationBuilder::new(&schema_id)
            .action(HeaderAction::Delete)
            .document_id(&document_id)
            .backlink(&backlink)
            .previous(&document_view_id)
            .depth(1)
            .sign(&key_pair)
            .is_ok());

        // DELETE operation must have non-zero depth
        assert!(OperationBuilder::new(&schema_id)
            .action(HeaderAction::Delete)
            .document_id(&document_id)
            .backlink(&backlink)
            .previous(&document_view_id)
            .depth(0)
            .sign(&key_pair)
            .is_err());

        // DELETE operations must not have fields
        assert!(OperationBuilder::new(&schema_id)
            .action(HeaderAction::Delete)
            .document_id(&document_id)
            .backlink(&backlink)
            .previous(&document_view_id)
            .fields(&[("year", 2020.into())])
            .depth(1)
            .sign(&key_pair)
            .is_err());

        // DELETE operations must have previous
        assert!(OperationBuilder::new(&schema_id)
            .action(HeaderAction::Delete)
            .document_id(&document_id)
            .backlink(&backlink)
            .depth(1)
            .sign(&key_pair)
            .is_err());

        // DELETE operations must have document id
        assert!(OperationBuilder::new(&schema_id)
            .action(HeaderAction::Delete)
            .backlink(&backlink)
            .previous(&document_view_id)
            .depth(1)
            .sign(&key_pair)
            .is_err());
    }

    #[rstest]
    fn field_ordering(key_pair: KeyPair, schema_id: SchemaId) {
        // Create first test operation
        let operation_1 = OperationBuilder::new(&schema_id)
            .timestamp(1703250169)
            .fields(&[("a", "sloth".into()), ("b", "penguin".into())])
            .sign(&key_pair);

        // Create second test operation with same values but different order of fields
        let operation_2 = OperationBuilder::new(&schema_id)
            .timestamp(1703250169)
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
