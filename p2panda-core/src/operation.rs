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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Header {
    /// Operation format version, allowing backwards compatibility when specification changes.
    pub version: u64,

    /// Author of this operation.
    pub public_key: PublicKey,

    /// Signature by author over all fields in header, providing authenticity.
    pub signature: Option<Signature>,

    /// Number of bytes of the body of this operation, can be omitted if no body is given.
    pub payload_size: u64,

    /// Hash of the body of this operation, can be omitted if no body is given.
    ///
    /// Keeping the hash here allows us to delete the payload (off-chain data) while retaining the
    /// ability to check the signature of the header.
    pub payload_hash: Option<Hash>,

    /// Time in microseconds since the Unix epoch.
    pub timestamp: u64,

    /// Number of operations this author has published to this log, begins with 0 and is
    /// always incremented by 1 with each new operation by the same author.
    pub seq_num: u64,

    /// Hash of the previous operation of the same author and log. Can be omitted if first
    /// operation in log.
    pub backlink: Option<Hash>,

    /// List of hashes of the operations we refer to as the "previous" ones. These are operations
    /// from other authors. Can be left empty if no partial ordering is required or no other author
    /// has been observed yet.
    pub previous: Vec<Hash>,
}

impl Header {
    fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::new();

        ciborium::ser::into_writer(&self, &mut bytes)
            // We can be sure that all values in this module are serializable and _if_ ciborium
            // still fails then because of something really bad ..
            .expect("CBOR encoder failed due to an critical IO error");

        bytes
    }

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
pub struct Body(pub(super) Vec<u8>);

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

pub fn validate_operation(operation: &Operation) -> Result<(), OperationError> {
    validate_header(&operation.header)?;

    let claimed_payload_size = operation.header.payload_size;
    let claimed_payload_hash: Option<Hash> = match claimed_payload_size {
        0 => None,
        _ => {
            let hash = operation
                .header
                .payload_hash
                .ok_or(OperationError::MissingPayloadHash)?;
            Some(hash)
        }
    };

    if let Some(body) = &operation.body {
        if claimed_payload_hash != Some(body.hash()) || claimed_payload_size != body.size() {
            return Err(OperationError::PayloadMismatch);
        }
    }

    Ok(())
}

pub fn validate_header(header: &Header) -> Result<(), OperationError> {
    if header.version != 1 {
        return Err(OperationError::UnsupportedVersion(header.version, 1));
    }

    if header.signature.is_none() {
        return Err(OperationError::MissingSignature);
    }

    if !header.verify() {
        return Err(OperationError::SignatureMismatch);
    }

    if (header.payload_hash.is_some() && header.payload_size == 0)
        || (header.payload_hash.is_none() && header.payload_size > 0)
    {
        return Err(OperationError::InconsistentPayloadInfo);
    }

    if !header.previous.is_empty() && header.backlink.is_none() {
        return Err(OperationError::LinksMismatch);
    }

    if header.backlink.is_some() && header.seq_num == 0 {
        return Err(OperationError::SeqNumMismatch);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::PrivateKey;

    use super::*;

    #[test]
    fn sign_and_verify() {
        let private_key = PrivateKey::new();
        let body = Body::new("Hello, Sloth!".as_bytes());

        let mut header = Header {
            version: 1,
            public_key: private_key.public_key(),
            signature: None,
            payload_size: body.size(),
            payload_hash: Some(body.hash()),
            timestamp: 0,
            seq_num: 0,
            backlink: None,
            previous: vec![],
        };
        assert!(!header.verify());

        header.sign(&private_key);
        assert!(header.verify());

        let operation = Operation {
            hash: header.hash(),
            header,
            body: Some(body),
        };
        assert!(validate_operation(&operation).is_ok());
    }
}
