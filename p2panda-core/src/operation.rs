// SPDX-License-Identifier: AGPL-3.0-or-later

use thiserror::Error;

use crate::hash::Hash;
use crate::identity::{PrivateKey, PublicKey, Signature};

#[derive(Clone, Debug)]
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
}

pub trait Encode {
    fn to_bytes(&self) -> Vec<u8>;
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
}

#[derive(Clone, Debug, PartialEq)]
pub struct Body(pub(crate) Vec<u8>);

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
