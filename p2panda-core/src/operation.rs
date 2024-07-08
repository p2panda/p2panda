// SPDX-License-Identifier: AGPL-3.0-or-later

use thiserror::Error;

use crate::hash::Hash;
use crate::identity::{PrivateKey, PublicKey, Signature};
use crate::Extensions;

#[derive(Clone, Debug)]
pub struct Operation<E>
where
    E: Extensions,
{
    pub hash: Hash,
    pub header: Header<E>,
    pub body: Option<Body>,
}

impl<E> PartialEq for Operation<E>
where
    E: Extensions,
{
    fn eq(&self, other: &Self) -> bool {
        self.hash.eq(&other.hash)
    }
}

impl<E> Eq for Operation<E> where E: Extensions {}

impl<E> PartialOrd for Operation<E>
where
    E: Extensions,
{
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.hash.cmp(&other.hash))
    }
}

impl<E> Ord for Operation<E>
where
    E: Extensions,
{
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.hash.cmp(&other.hash)
    }
}

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub struct Header<E>
where
    E: Extensions,
{
    /// Operation format version, allowing backwards compatibility when specification changes.
    pub version: u64,

    /// Author of this operation.
    pub public_key: PublicKey,

    /// Signature by author over all fields in header, providing authenticity.
    pub signature: Option<Signature>,

    /// Number of bytes of the body of this operation, must be zero if no body is given.
    pub payload_size: u64,

    /// Hash of the body of this operation, must be included if payload_size is non-zero and
    /// omitted otherwise.
    ///
    /// Keeping the hash here allows us to delete the payload (off-chain data) while retaining the
    /// ability to check the signature of the header.
    pub payload_hash: Option<Hash>,

    /// Time in microseconds since the Unix epoch.
    pub timestamp: u64,

    /// Number of operations this author has published to this log, begins with 0 and is always
    /// incremented by 1 with each new operation by the same author.
    pub seq_num: u64,

    /// Hash of the previous operation of the same author and log. Can be omitted if first
    /// operation in log.
    pub backlink: Option<Hash>,

    /// List of hashes of the operations we refer to as the "previous" ones. These are operations
    /// from other authors. Can be left empty if no partial ordering is required or no other author
    /// has been observed yet.
    pub previous: Vec<Hash>,

    /// Custom meta data.
    pub extensions: Option<E>,
}

impl<E> Header<E>
where
    E: Extensions,
{
    pub fn to_bytes(&self) -> Vec<u8> {
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

    #[error("sequence number can't be 0 when backlink is given")]
    SeqNumMismatch,

    #[error("payload hash and -size need to be defined together")]
    InconsistentPayloadInfo,

    #[error("needs payload hash in header when body is given")]
    MissingPayloadHash,

    #[error("payload hash and size do not match given body")]
    PayloadMismatch,

    #[error("logs can not contain operations of different authors")]
    TooManyAuthors,

    #[error("expected sequence number {0} but found {1}")]
    SeqNumNonIncremental(u64, u64),

    #[error("expected backlink but none was given")]
    BacklinkMissing,

    #[error("given backlink did not match previous operation")]
    BacklinkMismatch,
}

pub fn validate_operation<E: Extensions>(operation: &Operation<E>) -> Result<(), OperationError> {
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

pub fn validate_header<E: Extensions>(header: &Header<E>) -> Result<(), OperationError> {
    if !header.verify() {
        return Err(OperationError::SignatureMismatch);
    }

    if header.version != 1 {
        return Err(OperationError::UnsupportedVersion(header.version, 1));
    }

    if (header.payload_hash.is_some() && header.payload_size == 0)
        || (header.payload_hash.is_none() && header.payload_size > 0)
    {
        return Err(OperationError::InconsistentPayloadInfo);
    }

    if header.backlink.is_some() && header.seq_num == 0 {
        return Err(OperationError::SeqNumMismatch);
    }

    if header.backlink.is_none() && header.seq_num > 0 {
        return Err(OperationError::BacklinkMissing);
    }

    Ok(())
}

pub fn validate_backlink<E>(
    past_header: &Header<E>,
    header: &Header<E>,
) -> Result<(), OperationError>
where
    E: Extensions,
{
    if past_header.public_key != header.public_key {
        return Err(OperationError::TooManyAuthors);
    }

    if past_header.seq_num + 1 != header.seq_num {
        return Err(OperationError::SeqNumNonIncremental(
            past_header.seq_num + 1,
            header.seq_num,
        ));
    }

    match header.backlink {
        Some(backlink) => {
            if past_header.hash() != backlink {
                return Err(OperationError::BacklinkMismatch);
            }
        }
        None => {
            return Err(OperationError::BacklinkMissing);
        }
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

        let mut header = Header::<()> {
            version: 1,
            public_key: private_key.public_key(),
            signature: None,
            payload_size: body.size(),
            payload_hash: Some(body.hash()),
            timestamp: 0,
            seq_num: 0,
            backlink: None,
            previous: vec![],
            extensions: None,
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

    #[test]
    fn valid_backlink_header() {
        let private_key = PrivateKey::new();

        let mut header_0 = Header::<()> {
            version: 1,
            public_key: private_key.public_key(),
            signature: None,
            payload_size: 0,
            payload_hash: None,
            timestamp: 0,
            seq_num: 0,
            backlink: None,
            previous: vec![],
            extensions: None,
        };
        header_0.sign(&private_key);
        assert!(validate_header(&header_0).is_ok());

        let mut header_1 = Header::<()> {
            version: 1,
            public_key: private_key.public_key(),
            signature: None,
            payload_size: 0,
            payload_hash: None,
            timestamp: 0,
            seq_num: 1,
            backlink: Some(header_0.hash()),
            previous: vec![],
            extensions: None,
        };
        header_1.sign(&private_key);
        assert!(validate_header(&header_1).is_ok());

        assert!(validate_backlink(&header_0, &header_1).is_ok());
    }

    #[test]
    fn invalid_operations() {
        let private_key = PrivateKey::new();
        let body = Body::new("Hello, Sloth!".as_bytes());

        let header_base = Header::<()> {
            version: 1,
            public_key: private_key.public_key(),
            signature: None,
            payload_size: body.size(),
            payload_hash: Some(body.hash()),
            timestamp: 0,
            seq_num: 0,
            backlink: None,
            previous: vec![],
            extensions: None,
        };

        // Incompatible operation format
        let mut header = header_base.clone();
        header.version = 0;
        header.sign(&private_key);
        assert!(matches!(
            validate_header(&header),
            Err(OperationError::UnsupportedVersion(0, 1))
        ));

        // Signature doesn't match public key
        let mut header = header_base.clone();
        header.public_key = PrivateKey::new().public_key();
        header.sign(&private_key);
        assert!(matches!(
            validate_header(&header),
            Err(OperationError::SignatureMismatch)
        ));

        // Backlink missing
        let mut header = header_base.clone();
        header.seq_num = 1;
        header.sign(&private_key);
        assert!(matches!(
            validate_header(&header),
            Err(OperationError::BacklinkMissing)
        ));

        // Backlink given but sequence number indicates none
        let mut header = header_base.clone();
        header.backlink = Some(Hash::new(vec![4, 5, 6]));
        header.sign(&private_key);
        assert!(matches!(
            validate_header(&header),
            Err(OperationError::SeqNumMismatch)
        ));

        // Payload size does not match
        let mut header = header_base.clone();
        header.payload_size = 11;
        header.sign(&private_key);
        assert!(matches!(
            validate_operation(&Operation {
                hash: header.hash(),
                header,
                body: Some(body.clone()),
            }),
            Err(OperationError::PayloadMismatch)
        ));

        // Payload hash does not match
        let mut header = header_base.clone();
        header.payload_hash = Some(Hash::new(vec![4, 5, 6]));
        header.sign(&private_key);
        assert!(matches!(
            validate_operation(&Operation {
                hash: header.hash(),
                header,
                body: Some(body.clone()),
            }),
            Err(OperationError::PayloadMismatch)
        ));
    }
}
