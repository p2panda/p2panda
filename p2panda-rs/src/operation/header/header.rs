// SPDX-License-Identifier: AGPL-3.0-or-later

use serde::{Deserialize, Serialize};

use crate::document::{DocumentId, DocumentViewId};
use crate::hash::Hash;
use crate::identity::{KeyPair, PublicKey, Signature};
use crate::operation::body::EncodedBody;
use crate::operation::header::action::HeaderAction;
use crate::operation::header::encode::sign_header;
use crate::operation::header::error::{HeaderBuilderError, ValidateHeaderError};
use crate::operation::header::validate::validate_document_links;
use crate::operation::header::SeqNum;
use crate::operation::traits::{Actionable, Identifiable, Timestamped, Sequenced, Verifiable, Authored};
use crate::operation::{OperationAction, OperationVersion};
use crate::schema::SchemaId;
use crate::Validate;

pub type PayloadHash = Hash;

pub type PayloadSize = u64;

pub type Timestamp = u64;

pub type Backlink = Hash;

pub type Previous = DocumentViewId;

pub type Tombstone = bool;

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct DocumentLinks(pub DocumentId, pub Previous);

impl DocumentLinks {
    pub fn document_id(&self) -> &DocumentId {
        &self.0
    }

    pub fn previous(&self) -> &Previous {
        &self.1
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct Header(
    pub OperationVersion,
    pub PublicKey,
    pub PayloadHash,
    pub PayloadSize,
    pub Timestamp,
    pub SeqNum,
    pub Option<Backlink>,
    pub Option<DocumentLinks>,
    pub Tombstone,
    pub HeaderExtension,
    #[serde(skip_serializing_if = "Option::is_none")] pub Option<Signature>,
);

impl Header {
    pub fn extension(&self) -> &HeaderExtension {
        &self.9
    }

    pub fn document_id(&self) -> Option<&DocumentId> {
        self.7.as_ref().map(DocumentLinks::document_id)
    }

    pub fn tombstone(&self) -> bool {
        self.8
    }
}

impl Validate for Header {
    type Error = ValidateHeaderError;

    fn validate(&self) -> Result<(), Self::Error> {
        let backlink = self.backlink();
        let document_links = &self.7;
        let tombstone = self.tombstone();
        let seq_num = self.seq_num();

        match (seq_num, backlink, document_links, tombstone) {
            // header for CREATE operation
            (seq_num, None, None, false) if seq_num.is_first() => Ok(()),
            // header for UPDATE operation
            (_, _, Some(_), false) => Ok(()),
            // header for DELETE operation
            (_, _, Some(_), true) => Ok(()),
            // invalid CREATE header with non-zero sequence number
            (_, None, None, false) => Err(ValidateHeaderError::CreateUnexpectedNonZeroSeqNum),
            // invalid UPDATE header with backlink but no document id or previous
            (_, Some(_), None, false) => {
                Err(ValidateHeaderError::UpdateExpectedDocumentIdAndPrevious)
            }
            // invalid DELETE header with backlink but no document id or previous
            (_, _, None, true) => Err(ValidateHeaderError::DeleteExpectedDocumentIdAndPrevious),
        }
    }
}


impl Authored for Header {
    /// The public key of the keypair which signed this data.
    fn public_key(&self) -> &PublicKey {
        &self.1
    }
}

impl Timestamped for Header {
    /// Timestamp
    fn timestamp(&self) -> u64 {
        self.4
    }
}

impl Sequenced for Header {
    /// Sequence number of this operation.
    fn seq_num(&self) -> SeqNum {
        self.5
    }
}

impl Verifiable for Header {
    /// The signature.
    fn signature(&self) -> Signature {
        // We never use an unsigned header outside of our API
        self.10
            .clone()
            .expect("signature needs to be given at this point")
    }

    /// Size size in bytes of the payload.
    fn payload_size(&self) -> u64 {
        self.3
    }

    /// Hash of the payload.
    fn payload_hash(&self) -> &Hash {
        &self.2
    }

    /// Hash of the preceding operation in an authors log, None if this is the first operation.
    fn backlink(&self) -> Option<&Hash> {
        self.6.as_ref()
    }
}

impl Actionable for Header {
    fn version(&self) -> OperationVersion {
        self.0
    }

    fn action(&self) -> OperationAction {
        let backlink = &self.6;
        let document_links = &self.7;
        let tombstone = self.8;

        match (backlink, document_links, tombstone) {
            // header for CREATE operation
            (None, None, false) => OperationAction::Create,
            // header for UPDATE operation
            (_, Some(_), false) => OperationAction::Update,
            // header for DELETE operation
            (_, Some(_), true) => OperationAction::Delete,
            // if validation was performed correctly then all other cases will not occur.
            (_, _, _) => unreachable!(),
        }
    }

    fn previous(&self) -> Option<&DocumentViewId> {
        self.7.as_ref().map(|links| &links.1)
    }
}

#[derive(Debug, Default, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct HeaderExtension {
    #[serde(rename = "s", skip_serializing_if = "Option::is_none")]
    pub schema_id: Option<SchemaId>,
}

#[derive(Clone, Debug, Default)]
pub struct HeaderBuilder {
    timestamp: Timestamp,
    seq_num: SeqNum,
    tombstone: Tombstone,
    document_id: Option<DocumentId>,
    backlink: Option<Backlink>,
    previous: Option<Previous>,
    extension: HeaderExtension,
}

impl HeaderBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn tombstone(mut self) -> Self {
        self.tombstone = true;
        self
    }

    pub fn seq_num(mut self, seq_num: &SeqNum) -> Self {
        self.seq_num = seq_num.clone();
        self
    }

    pub fn timestamp(mut self, timestamp: u64) -> Self {
        self.timestamp = timestamp;
        self
    }

    pub fn previous(mut self, previous: &DocumentViewId) -> Self {
        self.previous = Some(previous.to_owned());
        self
    }

    pub fn schema_id(mut self, schema_id: &SchemaId) -> Self {
        self.extension.schema_id = Some(schema_id.to_owned());
        self
    }

    pub fn backlink(mut self, backlink: &Hash) -> Self {
        self.backlink = Some(backlink.to_owned());
        self
    }

    pub fn document_id(mut self, document_id: &DocumentId) -> Self {
        self.document_id = Some(document_id.to_owned());
        self
    }

    pub fn sign(
        self,
        encoded_body: &EncodedBody,
        key_pair: &KeyPair,
    ) -> Result<Header, HeaderBuilderError> {
        let document_links = validate_document_links(self.document_id, self.previous)?;
        let header = sign_header(
            self.timestamp,
            self.seq_num,
            self.backlink,
            document_links,
            self.tombstone,
            self.extension,
            encoded_body,
            key_pair,
        )?;
        header.validate()?;
        Ok(header)
    }
}
