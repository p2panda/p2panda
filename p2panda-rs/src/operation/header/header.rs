// SPDX-License-Identifier: AGPL-3.0-or-later

use serde::{Deserialize, Serialize};

use crate::document::{DocumentId, DocumentViewId};
use crate::hash::Hash;
use crate::identity::{KeyPair, PublicKey, Signature};
use crate::operation::body::EncodedBody;
use crate::operation::header::action::HeaderAction;
use crate::operation::header::encode::sign_header;
use crate::operation::header::error::EncodeHeaderError;
use crate::operation::header::traits::{Authored, Actionable};
use crate::operation::{OperationVersion, OperationAction};

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
}

impl Authored for Header {
    fn public_key(&self) -> &PublicKey {
        &self.1
    }

    fn payload_hash(&self) -> &Hash {
        &self.2
    }

    fn payload_size(&self) -> u64 {
        self.3
    }

    fn signature(&self) -> Signature {
        // We never use an unsigned header outside of our API
        self.5
            .clone()
            .expect("signature needs to be given at this point")
    }
}


impl Actionable for Header {
    fn version(&self) -> OperationVersion {
        self.0
    }

    fn action(&self) -> OperationAction {
        let HeaderExtension {
            action, previous, ..
        } = self.extension();
        match (action, previous) {
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
    #[serde(rename = "s", skip_serializing_if = "Option::is_none")]
    pub(crate) seq_num: Option<u64>,

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

    pub fn seq_num(mut self, seq_num: u64) -> Self {
        self.0.seq_num = Some(seq_num);
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
        sign_header(self.0, encoded_body, key_pair)
    }
}
