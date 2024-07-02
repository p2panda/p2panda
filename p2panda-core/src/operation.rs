// SPDX-License-Identifier: AGPL-3.0-or-later

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::fmt;

use serde::de::{Error as SerdeError, SeqAccess, Visitor};
use serde::ser::SerializeSeq;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::hash::Hash;
use crate::identity::{PrivateKey, PublicKey, Signature};
use crate::serde::{deserialize_hex, serialize_hex};

#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Operation {
    pub hash: Hash,
    pub header: Header,
    pub body: Option<Body>,
}

impl PartialEq for Operation {
    fn eq(&self, other: &Self) -> bool {
        self.hash.eq(&other.hash)
    }
}

impl Eq for Operation {}

impl PartialOrd for Operation {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.hash.cmp(&other.hash))
    }
}

impl Ord for Operation {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.hash.cmp(&other.hash)
    }
}

#[derive(Clone, PartialEq, Debug)]
pub struct Header {
    pub version: u64,
    pub public_key: PublicKey,
    pub signature: Option<Signature>,
    pub payload_hash: Option<Hash>,
    pub payload_size: u64,
    pub timestamp: u64,
    pub seq_num: u64,
    pub backlink: Option<Hash>,
    pub previous: Vec<Hash>,
    pub extensions: Option<Extensions>,
}

pub trait Encode {
    fn to_bytes(&self) -> Vec<u8>;
}

#[cfg(feature = "cbor")]
impl Encode for Header {
    fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();

        ciborium::ser::into_writer(&self, &mut bytes)
            // We can be sure that all values in this module are serializable and _if_ ciborium
            // still fails then because of something really bad ..
            .expect("CBOR encoder failed due to an critical IO error");

        bytes
    }
}

impl Header {
    pub fn sign(&mut self, private_key: &PrivateKey) {
        // Make sure the signature is not already set before we encode
        self.signature = None;

        let bytes = self.to_bytes();
        self.signature = Some(private_key.sign(&bytes));
    }

    pub fn verify(&self) -> bool {
        match self.signature {
            Some(claimed_signature) => {
                let mut unsigned_header = self.clone();
                unsigned_header.signature = None;
                let unsigned_bytes = unsigned_header.to_bytes();
                self.public_key.verify(&unsigned_bytes, &claimed_signature)
            }
            None => false,
        }
    }

    pub fn hash(&self) -> Hash {
        Hash::new(self.to_bytes())
    }

    pub fn extension(&self, key: &str) -> Option<&Extension> {
        match &self.extensions {
            Some(extensions) => extensions.get(key),
            None => None,
        }
    }
}

#[cfg(feature = "serde")]
impl Serialize for Header {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut seq = serializer.serialize_seq(None)?;
        seq.serialize_element(&self.version)?;
        seq.serialize_element(&self.public_key)?;

        if let Some(signature) = &self.signature {
            seq.serialize_element(signature)?;
        }

        seq.serialize_element(&self.payload_size)?;
        seq.serialize_element(&self.payload_hash)?;
        seq.serialize_element(&self.timestamp)?;
        seq.serialize_element(&self.seq_num)?;

        if let Some(backlink) = &self.backlink {
            seq.serialize_element(backlink)?;
        }

        seq.serialize_element(&self.previous)?;

        if let Some(extensions) = &self.extensions {
            seq.serialize_element(extensions)?;
        }

        seq.end()
    }
}

#[cfg(feature = "serde")]
impl<'de> Deserialize<'de> for Header {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct HeaderVisitor;

        impl<'de> Visitor<'de> for HeaderVisitor {
            type Value = Header;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("Header encoded as a sequence")
            }

            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                let version: u64 = seq
                    .next_element()
                    .map_err(|_| SerdeError::custom("invalid version, expected u64"))?
                    .ok_or(SerdeError::custom("version missing"))?;

                let public_key: PublicKey = seq
                    .next_element()
                    .map_err(|_| SerdeError::custom("invalid public key, expected bytes"))?
                    .ok_or(SerdeError::custom("public key missing"))?;

                let signature: Signature = seq
                    .next_element()
                    .map_err(|_| SerdeError::custom("invalid signature, expected bytes"))?
                    .ok_or(SerdeError::custom("signature missing"))?;

                let payload_size: u64 = seq
                    .next_element()
                    .map_err(|_| SerdeError::custom("invalid payload size, expected u64"))?
                    .ok_or(SerdeError::custom("payload size missing"))?;

                let payload_hash: Option<Hash> = match payload_size {
                    0 => None,
                    _ => {
                        let hash: Hash = seq
                            .next_element()
                            .map_err(|_| {
                                SerdeError::custom("invalid payload hash, expected bytes")
                            })?
                            .ok_or(SerdeError::custom("payload hash missing"))?;
                        Some(hash)
                    }
                };

                let timestamp: u64 = seq
                    .next_element()
                    .map_err(|_| SerdeError::custom("invalid timestamp, expected u64"))?
                    .ok_or(SerdeError::custom("timestamp missing"))?;

                let seq_num: u64 = seq
                    .next_element()
                    .map_err(|_| SerdeError::custom("invalid sequence number, expected u64"))?
                    .ok_or(SerdeError::custom("sequence number missing"))?;

                let backlink: Option<Hash> = match seq_num {
                    0 => None,
                    _ => {
                        let hash: Hash = seq
                            .next_element()
                            .map_err(|err| {
                                SerdeError::custom(format!(
                                    "invalid backlink, expected bytes {err}"
                                ))
                            })?
                            .ok_or(SerdeError::custom("backlink missing"))?;
                        Some(hash)
                    }
                };

                let previous: Vec<Hash> = seq
                    .next_element()
                    .map_err(|_| SerdeError::custom("invalid previous links, expected array"))?
                    .ok_or(SerdeError::custom("previous array missing"))?;

                let extensions: Option<Extensions> = seq
                    .next_element()
                    .map_err(|err| SerdeError::custom(format!("invalid extensions map: {err}")))?;

                Ok(Header {
                    version,
                    public_key,
                    signature: Some(signature),
                    payload_hash,
                    payload_size,
                    timestamp,
                    seq_num,
                    backlink,
                    previous,
                    extensions,
                })
            }
        }

        deserializer.deserialize_seq(HeaderVisitor)
    }
}

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Body(
    #[cfg_attr(
        feature = "serde",
        serde(serialize_with = "serialize_hex", deserialize_with = "deserialize_hex")
    )]
    Vec<u8>,
);

impl Body {
    pub fn new(bytes: &[u8]) -> Self {
        Self(bytes.to_vec())
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        self.0.clone()
    }

    pub fn hash(&self) -> Hash {
        Hash::new(&self.0)
    }

    pub fn size(&self) -> u64 {
        self.0.len() as u64
    }
}

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum Extension {
    Boolean(bool),
    Bytes(Vec<u8>),
    Float(f64),
    Integer(i64),
    String(String),
}

impl From<bool> for Extension {
    fn from(value: bool) -> Self {
        Extension::Boolean(value)
    }
}

impl From<&[u8]> for Extension {
    fn from(value: &[u8]) -> Self {
        Extension::Bytes(value.to_vec())
    }
}

impl From<Vec<u8>> for Extension {
    fn from(value: Vec<u8>) -> Self {
        Extension::Bytes(value)
    }
}

impl From<f64> for Extension {
    fn from(value: f64) -> Self {
        Extension::Float(value)
    }
}

impl From<i64> for Extension {
    fn from(value: i64) -> Self {
        Extension::Integer(value)
    }
}

impl From<&str> for Extension {
    fn from(value: &str) -> Self {
        Extension::String(value.to_string())
    }
}

impl From<String> for Extension {
    fn from(value: String) -> Self {
        Extension::String(value)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Extensions(BTreeMap<String, Extension>);

impl Default for Extensions {
    fn default() -> Self {
        Self::new()
    }
}

impl Extensions {
    pub fn new() -> Self {
        Self(BTreeMap::new())
    }

    pub fn insert(&mut self, key: &str, value: Extension) {
        self.0.insert(key.to_string(), value);
    }

    pub fn contains_key(&self, key: &str) -> bool {
        self.0.contains_key(key)
    }

    pub fn get(&self, key: &str) -> Option<&Extension> {
        self.0.get(key)
    }
}

#[cfg(feature = "serde")]
impl<'de> Deserialize<'de> for Extensions {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct ExtensionsVisitor;

        impl<'de> Visitor<'de> for ExtensionsVisitor {
            type Value = Extensions;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("canonical operation extensions map")
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::MapAccess<'de>,
            {
                let mut extensions = Extensions::new();
                let mut last_key: String = String::new();

                while let Some(key) = map.next_key::<String>()? {
                    // Check that field names are sorted lexicographically to ensure canonical
                    // encoding
                    if last_key.cmp(&key) == Ordering::Greater {
                        return Err(serde::de::Error::custom(format!(
                            "encountered unsorted extension key: '{}' should be before '{}'",
                            key, last_key,
                        )));
                    }

                    let value: Extension = map.next_value()?;
                    if extensions.0.insert(key.to_string(), value).is_some() {
                        // Fail if field names are duplicate to ensure canonical encoding
                        return Err(serde::de::Error::custom(format!(
                            "encountered duplicate extension key '{}'",
                            key
                        )));
                    }

                    last_key = key;
                }

                Ok(extensions)
            }
        }

        deserializer.deserialize_map(ExtensionsVisitor)
    }
}

#[derive(Error, Debug)]
pub enum OperationError {
    #[error("operation version {0} is not supported, needs to be <= {1}")]
    UnsupportedVersion(u64, u64),

    #[error("operation needs to be signed")]
    MissingSignature,

    #[error("signature does not match claimed public key")]
    SignatureMismatch,

    #[error("backlink needs to be set when previous link is used")]
    LinksMismatch,

    #[error("sequence number can't be 0 when backlink is given")]
    SeqNumMismatch,

    #[error("payload hash and -size need to be defined together")]
    InconsistentPayloadInfo,

    #[error("needs payload hash in header when body is given")]
    MissingPayloadHash,

    #[error("payload hash and size do not match given body")]
    PayloadMismatch,
}
