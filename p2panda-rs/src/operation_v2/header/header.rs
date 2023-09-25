// SPDX-License-Identifier: AGPL-3.0-or-later

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::document::DocumentViewId;
use crate::hash_v2::Hash;
use crate::identity_v2::{KeyPair, PublicKey, Signature};
use crate::operation_v2::body::EncodedBody;
use crate::operation_v2::header::encode::sign_header;
use crate::operation_v2::header::error::EncodeHeaderError;
use crate::operation_v2::header::traits::AsHeader;

pub type PayloadHash = Hash;

pub type PayloadSize = u64;

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct Header(
    pub(crate) HeaderVersion,
    pub(crate) PublicKey,
    pub(crate) PayloadHash,
    pub(crate) PayloadSize,
    pub(crate) HeaderExtension,
    #[serde(skip_serializing_if = "Option::is_none")] pub(crate) Option<Signature>,
);

impl AsHeader for Header {
    fn version(&self) -> HeaderVersion {
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

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum HeaderVersion {
    V1,
}

impl HeaderVersion {
    pub fn as_u64(&self) -> u64 {
        match self {
            HeaderVersion::V1 => 1,
        }
    }
}

impl Serialize for HeaderVersion {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u64(self.as_u64())
    }
}

impl<'de> Deserialize<'de> for HeaderVersion {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let version = u64::deserialize(deserializer)?;

        match version {
            1 => Ok(HeaderVersion::V1),
            _ => Err(serde::de::Error::custom(format!(
                "unsupported operation header version {}",
                version
            ))),
        }
    }
}

#[derive(Debug, Default, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct HeaderExtension {
    #[serde(rename = "s", skip_serializing_if = "Option::is_none")]
    seq_num: Option<u64>,

    #[serde(rename = "p", skip_serializing_if = "Option::is_none")]
    previous: Option<DocumentViewId>,

    #[serde(rename = "t", skip_serializing_if = "Option::is_none")]
    timestamp: Option<u64>,
}

#[derive(Clone, Debug, Default)]
pub struct HeaderBuilder(HeaderExtension);

impl HeaderBuilder {
    pub fn new() -> Self {
        Self::default()
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
