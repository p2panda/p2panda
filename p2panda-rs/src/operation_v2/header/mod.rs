// SPDX-License-Identifier: AGPL-3.0-or-later

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::document::DocumentViewId;
use crate::hash_v2::Hash;
use crate::identity_v2::{PublicKey, Signature};

pub type PayloadHash = Hash;

pub type PayloadSize = u64;

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
struct Header(
    HeaderVersion,
    PublicKey,
    PayloadHash,
    PayloadSize,
    HeaderExtension,
    Signature,
);

#[derive(Debug, Clone, Eq, PartialEq)]
enum HeaderVersion {
    V1,
}

impl HeaderVersion {
    /// Returns the operation version encoded as u64.
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

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
struct HeaderExtension {
    #[serde(rename = "s", skip_serializing_if = "Option::is_none")]
    seq_num: Option<u64>,

    #[serde(rename = "p", skip_serializing_if = "Option::is_none")]
    previous: Option<DocumentViewId>,

    #[serde(rename = "t", skip_serializing_if = "Option::is_none")]
    timestamp: Option<u64>,
}
