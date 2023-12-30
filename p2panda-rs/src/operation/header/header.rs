// SPDX-License-Identifier: AGPL-3.0-or-later

use serde::{Deserialize, Serialize};

use crate::document::{DocumentId, DocumentViewId};
use crate::hash::Hash;
use crate::identity::{KeyPair, PublicKey, Signature};
use crate::operation::body::EncodedBody;
use crate::operation::header::action::HeaderAction;
use crate::operation::header::encode::sign_header;
use crate::operation::header::error::EncodeHeaderError;
use crate::operation::traits::Actionable;
use crate::operation::{OperationAction, OperationVersion};
use crate::Validate;

use super::error::ValidateHeaderError;

pub type PayloadHash = Hash;

pub type PayloadSize = u64;

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct Header(
    pub(crate) OperationVersion,
    pub(crate) PublicKey,
    pub(crate) PayloadHash,
    pub(crate) PayloadSize,
    pub(crate) HeaderExtension,
    #[serde(skip_serializing_if = "Option::is_none")] pub(crate) Option<Signature>,
);

impl Header {
    pub fn extension(&self) -> &HeaderExtension {
        &self.4
    }

    pub fn public_key(&self) -> &PublicKey {
        &self.1
    }

    pub fn payload_hash(&self) -> &Hash {
        &self.2
    }

    pub fn payload_size(&self) -> u64 {
        self.3
    }

    pub fn signature(&self) -> Signature {
        // We never use an unsigned header outside of our API
        self.5
            .clone()
            .expect("signature needs to be given at this point")
    }
}

impl Validate for Header {
    type Error = ValidateHeaderError;

    fn validate(&self) -> Result<(), Self::Error> {
        // The validation performed here is based on only the strictest requirements expected
        // of a header. It is possible to build headers which may be incompatible with the current
        // p2panda operation specification. We intentionally don't enforce these restrictions here
        // in order to leave the option to publish custom operation header formats open.

        // What is validated here:
        // - if a document id is not present, then we know this is a header for a CREATE operation
        //   and should therefore _not_ contain a backlink or previous extension as well.
        // - if action is DELETE then a document id _must_ also be provided.

        let HeaderExtension {
            action,
            previous,
            backlink,
            document_id,
            ..
        } = &self.4;

        match (document_id, backlink, previous) {
            (None, Some(_), _) => Err(ValidateHeaderError::CreateUnexpectedBacklink),
            (None, None, Some(_)) => Err(ValidateHeaderError::CreateUnexpectedPrevious),
            (_, _, _) => Ok(()),
        }?;

        match (document_id, action) {
            (None, Some(HeaderAction::Delete)) => {
                Err(ValidateHeaderError::DeleteExpectedDocumentId)
            }
            (_, _) => Ok(()),
        }
    }
}

impl Actionable for Header {
    fn version(&self) -> OperationVersion {
        self.0
    }

    fn action(&self) -> OperationAction {
        let HeaderExtension {
            action,
            document_id,
            ..
        } = self.extension();

        // Action
        match (action, document_id) {
            (None, None) => OperationAction::Create,
            (None, Some(_)) => OperationAction::Update,
            (Some(HeaderAction::Delete), Some(_)) => OperationAction::Delete,
            // If correct validation was performed this case will not occur.
            (Some(HeaderAction::Delete), None) => unreachable!(),
        }
    }

    fn previous(&self) -> Option<&DocumentViewId> {
        self.extension().previous.as_ref()
    }
}

#[derive(Debug, Default, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct HeaderExtension {
    #[serde(rename = "h", skip_serializing_if = "Option::is_none")]
    pub(crate) depth: Option<u64>,

    #[serde(rename = "d", skip_serializing_if = "Option::is_none")]
    pub(crate) document_id: Option<DocumentId>,

    #[serde(rename = "p", skip_serializing_if = "Option::is_none")]
    pub(crate) previous: Option<DocumentViewId>,

    #[serde(rename = "b", skip_serializing_if = "Option::is_none")]
    pub(crate) backlink: Option<Hash>,

    #[serde(rename = "a", skip_serializing_if = "Option::is_none")]
    pub(crate) action: Option<HeaderAction>,

    #[serde(rename = "t", skip_serializing_if = "Option::is_none")]
    pub(crate) timestamp: Option<u128>,
}

#[derive(Clone, Debug, Default)]
pub struct HeaderBuilder(HeaderExtension);

impl HeaderBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn action(mut self, action: HeaderAction) -> Self {
        self.0.action = Some(action);
        self
    }

    pub fn depth(mut self, depth: u64) -> Self {
        self.0.depth = Some(depth);
        self
    }

    pub fn timestamp(mut self, timestamp: u128) -> Self {
        self.0.timestamp = Some(timestamp);
        self
    }

    pub fn previous(mut self, previous: &DocumentViewId) -> Self {
        self.0.previous = Some(previous.to_owned());
        self
    }

    pub fn backlink(mut self, backlink: &Hash) -> Self {
        self.0.backlink = Some(backlink.to_owned());
        self
    }

    pub fn document_id(mut self, document_id: &DocumentId) -> Self {
        self.0.document_id = Some(document_id.to_owned());
        self
    }

    pub fn sign(
        self,
        encoded_body: &EncodedBody,
        key_pair: &KeyPair,
    ) -> Result<Header, EncodeHeaderError> {
        let header = sign_header(self.0, encoded_body, key_pair)?;
        header.validate()?;
        Ok(header)
    }
}
