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
use crate::operation::header::validate::validate_document_links;
use crate::operation::header::{Header, HeaderExtension, SeqNum};
use crate::operation::traits::{
    Actionable, Authored, Capable, Fielded, Identifiable, Payloaded, Timestamped,
};
use crate::operation::{
    OperationAction, OperationFields, OperationId, OperationValue, OperationVersion,
};
use crate::schema::SchemaId;
use crate::Validate;

#[derive(Clone, Debug, PartialEq)]
pub struct Operation(pub OperationId, pub Header, pub Body);

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

#[derive(Clone, Debug, Default)]
pub struct OperationBuilder {
    timestamp: u64,
    seq_num: SeqNum,
    backlink: Option<Hash>,
    document_id: Option<DocumentId>,
    previous: Option<DocumentViewId>,
    tombstone: bool,
    header_extension: HeaderExtension,
    body: Body,
}

impl Validate for Operation {
    type Error = ValidateOperationError;

    fn validate(&self) -> Result<(), Self::Error> {
        // Check the header represents a valid operation action.
        self.header().validate()?;

        // Check the header contains a schema id extension.
        if self.header().extension().schema_id.is_none() {
            return Err(ValidateOperationError::ExpectedFields);
        }

        // Check that fields are provided when expected.
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
    pub fn new(schema_id: &SchemaId, timestamp: u64) -> Self {
        let mut header_extension = HeaderExtension::default();
        header_extension.schema_id = Some(schema_id.to_owned());

        let body = Body(None);

        Self {
            timestamp,
            header_extension,
            body,
            ..Default::default()
        }
    }

    /// Set operation schema.
    pub fn schema_id(mut self, schema_id: SchemaId) -> Self {
        self.header_extension.schema_id = Some(schema_id);
        self
    }

    /// Set operation backlink.
    pub fn backlink(mut self, backlink: &Hash) -> Self {
        self.backlink = Some(backlink.to_owned());
        self
    }

    /// Set previous operations.
    pub fn previous(mut self, previous: &DocumentViewId) -> Self {
        self.previous = Some(previous.to_owned());
        self
    }

    /// Set unix timestamp in nanoseconds.
    pub fn timestamp(mut self, timestamp: u64) -> Self {
        self.timestamp = timestamp;
        self
    }

    /// Set document id.
    pub fn document_id(mut self, document_id: &DocumentId) -> Self {
        self.document_id = Some(document_id.to_owned());
        self
    }

    /// Set seq_num.
    pub fn seq_num(mut self, seq_num: u64) -> Self {
        self.seq_num = SeqNum::new(seq_num);
        self
    }

    /// Set tombstone to true.
    pub fn tombstone(mut self) -> Self {
        self.tombstone = true;
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

        self.body.0 = Some(operation_fields);
        self
    }

    /// Builds and returns a new `Operation` instance.
    ///
    /// This method checks if the given previous operations and operation fields are matching the
    /// regarding operation action.
    pub fn sign(self, key_pair: &KeyPair) -> Result<Operation, OperationBuilderError> {
        let document_links = validate_document_links(self.document_id, self.previous)?;
        let payload = encode_body(&self.body)?;
        let header = sign_header(
            self.timestamp,
            self.seq_num,
            self.backlink,
            document_links,
            self.tombstone,
            self.header_extension,
            &payload,
            key_pair,
        )?;
        let header_hash = encode_header(&header)?.hash();
        let operation = Operation::new(header_hash.into(), header, self.body)?;
        Ok(operation)
    }
}

impl Identifiable for Operation {
    /// Id of this operation.
    fn id(&self) -> &OperationId {
        &self.0
    }

    /// Id of the document this operation belongs to.
    fn document_id(&self) -> DocumentId {
        match self.header().document_id() {
            Some(document_id) => document_id.clone(),
            None => DocumentId::new(self.id()),
        }
    }
}

impl Timestamped for Operation {
    /// Timestamp
    fn timestamp(&self) -> u64 {
        self.header().timestamp()
    }
}

impl Capable for Operation {
    /// Hash of the preceding operation in an authors log, None if this is the first operation.
    fn backlink(&self) -> Option<&Hash> {
        self.header().backlink()
    }

    /// Sequence number of this operation.
    fn seq_num(&self) -> SeqNum {
        // Safely unwrap as validation performed already.
        self.header().seq_num()
    }
}

impl Actionable for Operation {
    /// Returns the operation version.
    fn version(&self) -> OperationVersion {
        self.header().0
    }

    /// Returns the operation action.
    fn action(&self) -> OperationAction {
        self.header().action()
    }

    /// Returns a list of previous operations.
    fn previous(&self) -> Option<&DocumentViewId> {
        self.header().previous()
    }
}

impl Fielded for Operation {
    /// Returns application data fields of operation.
    fn fields(&self) -> Option<&OperationFields> {
        self.body().0.as_ref()
    }
}

impl Schematic for Operation {
    /// Returns the schema id of this operation.
    fn schema_id(&self) -> &SchemaId {
        // Safely unwrap as extension validation is expected to have already been performed.
        self.header().extension().schema_id.as_ref().unwrap()
    }

    /// Returns the fields of this operation in plain form.
    fn plain_fields(&self) -> Option<PlainFields> {
        self.body().plain_fields()
    }
}

impl Authored for Operation {
    /// The public key of the keypair which signed this data.
    fn public_key(&self) -> &crate::identity::PublicKey {
        self.header().public_key()
    }

    /// The signature.
    fn signature(&self) -> crate::identity::Signature {
        self.header().signature()
    }
}

impl Payloaded for Operation {
    /// Size size in bytes of the payload.
    fn payload_size(&self) -> u64 {
        self.header().payload_size()
    }

    /// Hash of the payload.
    fn payload_hash(&self) -> &Hash {
        self.header().payload_hash()
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use crate::document::{DocumentId, DocumentViewId};
    use crate::hash::Hash;
    use crate::identity::KeyPair;
    use crate::operation::body::traits::Schematic;
    use crate::operation::header::HeaderAction;
    use crate::operation::traits::{Actionable, Capable, Fielded, Identifiable, Timestamped};
    use crate::operation::{
        OperationAction, OperationBuilder, OperationFields, OperationValue, OperationVersion,
    };
    use crate::schema::SchemaId;
    use crate::test_utils::constants::TIMESTAMP;
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

        let operation = OperationBuilder::new(&schema_id, TIMESTAMP)
            .fields(&fields)
            .sign(&key_pair)
            .unwrap();

        assert_eq!(operation.version(), OperationVersion::V1);
        assert_eq!(operation.action(), OperationAction::Create);
        assert_eq!(operation.schema_id(), &schema_id);
        assert_eq!(operation.document_id(), DocumentId::new(operation.id()));
        assert_eq!(operation.backlink(), None);
        assert_eq!(operation.previous(), None);
        assert_eq!(operation.seq_num(), 0.into());
        assert_eq!(operation.timestamp(), TIMESTAMP);
        assert_eq!(operation.fields(), Some(&fields.into()));
    }

    #[rstest]
    fn operation_builder_update(
        key_pair: KeyPair,
        schema_id: SchemaId,
        document_id: DocumentId,
        document_view_id: DocumentViewId,
    ) {
        let fields = vec![
            ("firstname", "Peter".into()),
            ("lastname", "Panda".into()),
            ("year", 2020.into()),
        ];

        let operation = OperationBuilder::new(&schema_id, TIMESTAMP)
            .document_id(&document_id)
            .previous(&document_view_id)
            .seq_num(1)
            .fields(&fields)
            .sign(&key_pair)
            .unwrap();

        assert_eq!(operation.version(), OperationVersion::V1);
        assert_eq!(operation.action(), OperationAction::Update);
        assert_eq!(operation.schema_id(), &schema_id);
        assert_eq!(operation.document_id(), document_id);
        assert_eq!(operation.previous(), Some(&document_view_id));
        assert_eq!(operation.seq_num(), 1.into());
        assert_eq!(operation.timestamp(), TIMESTAMP);
        assert_eq!(operation.fields(), Some(&fields.into()));
    }

    #[rstest]
    fn operation_builder_delete(
        key_pair: KeyPair,
        schema_id: SchemaId,
        document_id: DocumentId,
        document_view_id: DocumentViewId,
    ) {
        let operation = OperationBuilder::new(&schema_id, TIMESTAMP)
            .document_id(&document_id)
            .previous(&document_view_id)
            .seq_num(1)
            .tombstone()
            .sign(&key_pair)
            .unwrap();

        assert_eq!(operation.version(), OperationVersion::V1);
        assert_eq!(operation.action(), OperationAction::Delete);
        assert_eq!(operation.schema_id(), &schema_id);
        assert_eq!(operation.document_id(), document_id);
        assert_eq!(operation.previous(), Some(&document_view_id));
        assert_eq!(operation.seq_num(), 1.into());
        assert_eq!(operation.timestamp(), TIMESTAMP);
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
        assert!(OperationBuilder::new(&schema_id, TIMESTAMP)
            .fields(&[("year", 2020.into())])
            .sign(&key_pair)
            .is_ok());

        // CREATE operations must not contain previous
        assert!(OperationBuilder::new(&schema_id, TIMESTAMP)
            .previous(&document_view_id)
            .fields(&[("year", 2020.into())])
            .sign(&key_pair)
            .is_err());

        // CREATE operations must not contain backlink
        assert!(OperationBuilder::new(&schema_id, TIMESTAMP)
            .backlink(&backlink)
            .fields(&[("year", 2020.into())])
            .sign(&key_pair)
            .is_err());

        // CREATE operations must not contain non-zero seq_num
        assert!(OperationBuilder::new(&schema_id, TIMESTAMP)
            .seq_num(1)
            .fields(&[("year", 2020.into())])
            .sign(&key_pair)
            .is_err());

        // CREATE operations must contain fields
        assert!(OperationBuilder::new(&schema_id, TIMESTAMP)
            .sign(&key_pair)
            .is_err());

        // correct UPDATE operation
        assert!(OperationBuilder::new(&schema_id, TIMESTAMP)
            .document_id(&document_id)
            .previous(&document_view_id)
            .seq_num(1)
            .fields(&[("year", 2020.into())])
            .sign(&key_pair)
            .is_ok());

        // UPDATE operation may contain backlink
        assert!(OperationBuilder::new(&schema_id, TIMESTAMP)
            .document_id(&document_id)
            .backlink(&backlink)
            .previous(&document_view_id)
            .seq_num(1)
            .fields(&[("year", 2020.into())])
            .sign(&key_pair)
            .is_ok());

        // UPDATE operations must have fields
        assert!(OperationBuilder::new(&schema_id, TIMESTAMP)
            .document_id(&document_id)
            .backlink(&backlink)
            .previous(&document_view_id)
            .seq_num(1)
            .sign(&key_pair)
            .is_err());

        // UPDATE operations must have previous
        assert!(OperationBuilder::new(&schema_id, TIMESTAMP)
            .document_id(&document_id)
            .backlink(&backlink)
            .seq_num(1)
            .fields(&[("year", 2020.into())])
            .sign(&key_pair)
            .is_err());

        // UPDATE operations must have document id
        assert!(OperationBuilder::new(&schema_id, TIMESTAMP)
            .backlink(&backlink)
            .previous(&document_view_id)
            .seq_num(1)
            .fields(&[("year", 2020.into())])
            .sign(&key_pair)
            .is_err());

        // correct DELETE operation
        assert!(OperationBuilder::new(&schema_id, TIMESTAMP)
            .document_id(&document_id)
            .previous(&document_view_id)
            .seq_num(1)
            .tombstone()
            .sign(&key_pair)
            .is_ok());

        // DELETE operation may contain backlink
        assert!(OperationBuilder::new(&schema_id, TIMESTAMP)
            .document_id(&document_id)
            .backlink(&backlink)
            .previous(&document_view_id)
            .tombstone()
            .seq_num(1)
            .sign(&key_pair)
            .is_ok());

        // DELETE operations must not have fields
        assert!(OperationBuilder::new(&schema_id, TIMESTAMP)
            .document_id(&document_id)
            .backlink(&backlink)
            .previous(&document_view_id)
            .tombstone()
            .fields(&[("year", 2020.into())])
            .seq_num(1)
            .sign(&key_pair)
            .is_err());

        // DELETE operations must have previous
        assert!(OperationBuilder::new(&schema_id, TIMESTAMP)
            .document_id(&document_id)
            .backlink(&backlink)
            .seq_num(1)
            .tombstone()
            .sign(&key_pair)
            .is_err());

        // DELETE operations must have document id
        assert!(OperationBuilder::new(&schema_id, TIMESTAMP)
            .backlink(&backlink)
            .previous(&document_view_id)
            .seq_num(1)
            .tombstone()
            .sign(&key_pair)
            .is_err());
    }

    #[rstest]
    fn field_ordering(key_pair: KeyPair, schema_id: SchemaId) {
        // Create first test operation
        let operation_1 = OperationBuilder::new(&schema_id, TIMESTAMP)
            .timestamp(1703250169)
            .fields(&[("a", "sloth".into()), ("b", "penguin".into())])
            .sign(&key_pair);

        // Create second test operation with same values but different order of fields
        let operation_2 = OperationBuilder::new(&schema_id, TIMESTAMP)
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
