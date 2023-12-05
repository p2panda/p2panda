// SPDX-License-Identifier: AGPL-3.0-or-later

use serde::{Deserialize, Serialize};

use crate::document::DocumentViewId;
use crate::hash_v2::Hash;
use crate::identity_v2::{KeyPair, PublicKey, Signature};
use crate::operation_v2::body::EncodedBody;
use crate::operation_v2::header::action::HeaderAction;
use crate::operation_v2::header::encode::sign_header;
use crate::operation_v2::header::error::EncodeHeaderError;
use crate::operation_v2::header::traits::{Authored, Actionable};
use crate::operation_v2::{OperationAction, OperationVersion};

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
        self.5.clone().expect("signature needs to be given at this point")
    }
}

impl Actionable for Header {
    fn version(&self) -> OperationVersion {
        self.0.to_owned()
    }

    fn action(&self) -> OperationAction {
        match (self.4.action, self.previous()) {
            (None, None) => OperationAction::Create,
            (None, Some(_)) => OperationAction::Update,
            (Some(HeaderAction::Delete), Some(_)) => OperationAction::Delete,
            // @TODO: This should never happen if we've validated it properly before?
            (Some(HeaderAction::Delete), None) => unreachable!("Invalid case"),
        }
    }

    fn previous(&self) -> Option<&DocumentViewId> {
        self.4.previous.as_ref()
    }
}

#[derive(Debug, Default, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct HeaderExtension {
    #[serde(rename = "s", skip_serializing_if = "Option::is_none")]
    pub(crate) seq_num: Option<u64>,

    #[serde(rename = "p", skip_serializing_if = "Option::is_none")]
    pub(crate) previous: Option<DocumentViewId>,

    #[serde(rename = "a", skip_serializing_if = "Option::is_none")]
    pub(crate) action: Option<HeaderAction>,

    #[serde(rename = "t", skip_serializing_if = "Option::is_none")]
    pub(crate) timestamp: Option<u64>,
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

    pub fn timestamp(mut self, timestamp: u64) -> Self {
        self.0.timestamp = Some(timestamp);
        self
    }

    pub fn previous(mut self, previous: &DocumentViewId) -> Self {
        self.0.previous = Some(previous.to_owned());
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
