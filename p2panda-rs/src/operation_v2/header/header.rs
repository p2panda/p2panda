// SPDX-License-Identifier: AGPL-3.0-or-later

use serde::{Deserialize, Serialize};

use crate::document::DocumentViewId;
use crate::hash_v2::Hash;
use crate::identity_v2::{KeyPair, PublicKey, Signature};
use crate::operation_v2::body::EncodedBody;
use crate::operation_v2::header::encode::sign_header;
use crate::operation_v2::header::error::EncodeHeaderError;
use crate::operation_v2::header::traits::AsHeader;
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

impl AsHeader for Header {
    fn version(&self) -> OperationVersion {
        self.0
    }

    fn public_key(&self) -> &PublicKey {
        &self.1
    }

    fn payload_hash(&self) -> &Hash {
        &self.2
    }

    fn payload_size(&self) -> u64 {
        self.3
    }

    fn extensions(&self) -> &HeaderExtension {
        &self.4
    }

    fn signature(&self) -> &Signature {
        // We never use an unsigned header outside of our API
        &self.5.expect("signature needs to be given at this point")
    }
}

#[derive(Debug, Default, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct HeaderExtension {
    #[serde(rename = "s", skip_serializing_if = "Option::is_none")]
    seq_num: Option<u64>,

    #[serde(rename = "p", skip_serializing_if = "Option::is_none")]
    previous: Option<DocumentViewId>,

    #[serde(rename = "a", skip_serializing_if = "Option::is_none")]
    action: Option<OperationAction>,

    #[serde(rename = "t", skip_serializing_if = "Option::is_none")]
    timestamp: Option<u64>,
}

#[derive(Clone, Debug, Default)]
pub struct HeaderBuilder(HeaderExtension);

impl HeaderBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn action(mut self, action: OperationAction) -> Self {
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
