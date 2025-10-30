// SPDX-License-Identifier: MIT OR Apache-2.0

//! Core p2panda data type offering distributed, secure and efficient data transfer between peers.
//!
//! Operations are used to carry any data from one peer to another (distributed), while assuming no
//! reliable network connection (offline-first) and untrusted machines (cryptographically secure).
//! The author of an operation uses it's [`PrivateKey`] to cryptographically sign every operation.
//! This can be verified and used for authentication by any other peer.
//!
//! Every operation consists of a [`Header`] and an optional [`Body`]. The body holds arbitrary
//! bytes (up to the application to decide what should be inside). The header is used to
//! cryptographically secure & authenticate the body and for providing ordered collections of
//! operations when required.
//!
//! Operations have a `backlink` and `seq_num` field in the header. These are used to form a linked
//! list of operations, where every subsequent operation points to the previous one by referencing
//! its cryptographically secured hash. The `previous` field can be used to point at operations by
//! _other_ authors when multi-writer causal partial-ordering is required. The `timestamp` field
//! can be used when verifiable causal ordering is not required.
//!
//! [Header extensions](crate::extensions) can be used to add additional information, like
//! "pruning" points for removing old or unwanted data, "tombstones" for explicit deletion,
//! capabilities or group encryption schemes or custom application-related features etc.
//!
//! Operations are encoded in CBOR format and use Ed25519 key pairs for digital signatures and
//! BLAKE3 for hashing.
//!
//! ## Examples
//!
//! ### Construct and sign a header
//!
//! ```
//! use p2panda_core::{Body, Header, PrivateKey};
//!
//! let private_key = PrivateKey::new();
//!
//! let body = Body::new("Hello, Sloth!".as_bytes());
//! let mut header = Header {
//!     version: 1,
//!     public_key: private_key.public_key(),
//!     signature: None,
//!     payload_size: body.size(),
//!     payload_hash: Some(body.hash()),
//!     timestamp: 1733170247,
//!     seq_num: 0,
//!     backlink: None,
//!     previous: vec![],
//!     extensions: (),
//! };
//!
//! header.sign(&private_key);
//! ```
//!
//! ### Custom extensions
//!
//! ```
//! use p2panda_core::{Body, Extension, Header, PrivateKey, PruneFlag};
//! use serde::{Serialize, Deserialize};
//!
//! let private_key = PrivateKey::new();
//!
//! #[derive(Clone, Debug, Default, Serialize, Deserialize)]
//! struct CustomExtensions {
//!     prune_flag: PruneFlag,
//! }
//!
//! impl Extension<PruneFlag> for CustomExtensions {
//!     fn extract(header: &Header<Self>) -> Option<PruneFlag> {
//!         Some(header.extensions.prune_flag.clone())
//!     }
//! }
//!
//! let extensions = CustomExtensions {
//!     prune_flag: PruneFlag::new(true),
//! };
//!
//! let body = Body::new("Prune from here please!".as_bytes());
//! let mut header = Header {
//!     version: 1,
//!     public_key: private_key.public_key(),
//!     signature: None,
//!     payload_size: body.size(),
//!     payload_hash: Some(body.hash()),
//!     timestamp: 1733170247,
//!     seq_num: 0,
//!     backlink: None,
//!     previous: vec![],
//!     extensions,
//! };
//!
//! header.sign(&private_key);
//!
//! let prune_flag: PruneFlag = header.extension().unwrap();
//! assert!(prune_flag.is_set())
//! ```
use thiserror::Error;

use crate::cbor::{DecodeError, decode_cbor, encode_cbor};
use crate::hash::Hash;
use crate::identity::{PrivateKey, PublicKey, Signature};
use crate::{Extension, Extensions};

/// Encoded bytes of an operation header and optional body.
pub type RawOperation = (Vec<u8>, Option<Vec<u8>>);

/// Combined [`Header`], [`Body`] and operation [`struct@Hash`] (Operation Id).
#[derive(Clone, Debug)]
pub struct Operation<E = ()> {
    pub hash: Hash,
    pub header: Header<E>,
    pub body: Option<Body>,
}

impl<E> PartialEq for Operation<E> {
    fn eq(&self, other: &Self) -> bool {
        self.hash.eq(&other.hash)
    }
}

impl<E> Eq for Operation<E> {}

impl<E> PartialOrd for Operation<E> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.hash.cmp(&other.hash))
    }
}

impl<E> Ord for Operation<E> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.hash.cmp(&other.hash)
    }
}

/// Header of a p2panda operation.
///
/// The header holds all metadata required to cryptographically secure and authenticate a message
/// [`Body`] and, if required, apply ordering to collections of messages from the same or many
/// authors.
///
/// ## Example
///
/// ```
/// use p2panda_core::{Body, Header, Operation, PrivateKey};
///
/// let private_key = PrivateKey::new();
///
/// let body = Body::new("Hello, Sloth!".as_bytes());
/// let mut header = Header {
///     version: 1,
///     public_key: private_key.public_key(),
///     signature: None,
///     payload_size: body.size(),
///     payload_hash: Some(body.hash()),
///     timestamp: 1733170247,
///     seq_num: 0,
///     backlink: None,
///     previous: vec![],
///     extensions: (),
/// };
///
/// // Sign the header with the author's private key.
/// header.sign(&private_key);
/// ```
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub struct Header<E = ()> {
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
    /// from other authors. Can be left empty if no partial ordering is required or no other
    /// author has been observed yet.
    pub previous: Vec<Hash>,

    /// Custom meta data.
    pub extensions: E,
}

impl<E: Default> Default for Header<E> {
    fn default() -> Self {
        Self {
            version: 1,
            public_key: PublicKey::default(),
            signature: None,
            payload_size: 0,
            payload_hash: None,
            timestamp: 0,
            seq_num: 0,
            backlink: None,
            previous: vec![],
            extensions: E::default(),
        }
    }
}

impl<E> Header<E>
where
    E: Extensions,
{
    /// Header encoded to bytes in CBOR format.
    pub fn to_bytes(&self) -> Vec<u8> {
        encode_cbor(self)
            // We can be sure that all values in this module are serializable and _if_ ciborium
            // still fails then because of something really bad ..
            .expect("CBOR encoder failed due to an critical IO error")
    }

    /// Add a signature to the header using the provided `PrivateKey`.
    ///
    /// This method signs the byte representation of a header with any existing signature removed
    /// before adding back the newly generated signature.
    pub fn sign(&mut self, private_key: &PrivateKey) {
        // Make sure the signature is not already set before we encode
        self.signature = None;

        let bytes = self.to_bytes();
        self.signature = Some(private_key.sign(&bytes));
    }

    /// Verify that the signature contained in this `Header` was generated by the claimed
    /// public key.
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

    /// BLAKE3 hash of the header bytes.
    ///
    /// This hash is used as the unique identifier of an operation, aka the Operation Id.
    pub fn hash(&self) -> Hash {
        Hash::new(self.to_bytes())
    }

    /// Extract an extension value from the header.
    pub fn extension<T>(&self) -> Option<T>
    where
        E: Extension<T>,
    {
        E::extract(self)
    }
}

impl<E> Header<E> {
    /// Number of fields included in the header.
    ///
    /// Fields instantiated with `None` values are excluded from the count.
    pub(crate) fn field_count(&self) -> usize {
        // There will always be a minimum of 7 fields in a complete header.
        // (this counts the `E` extensions field, even if it is zero-sized).
        let mut count = 7;

        if self.signature.is_some() {
            count += 1;
        }

        if self.payload_hash.is_some() {
            count += 1;
        }

        if self.backlink.is_some() {
            count += 1;
        }

        count
    }
}

impl TryFrom<&[u8]> for Header {
    type Error = DecodeError;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        decode_cbor(value)
    }
}

/// Body of a p2panda operation containing arbitrary bytes.
#[derive(Clone, Debug, PartialEq)]
pub struct Body(pub(super) Vec<u8>);

impl Body {
    /// Construct a body from a byte slice.
    pub fn new(bytes: &[u8]) -> Self {
        Self(bytes.to_vec())
    }

    /// Access the underlying body bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        self.0.clone()
    }

    /// BLAKE3 hash of the body bytes.
    pub fn hash(&self) -> Hash {
        Hash::new(&self.0)
    }

    /// Size of body bytes.
    pub fn size(&self) -> u64 {
        self.0.len() as u64
    }
}

impl From<&[u8]> for Body {
    fn from(value: &[u8]) -> Self {
        Body::new(value)
    }
}

impl From<Vec<u8>> for Body {
    fn from(value: Vec<u8>) -> Self {
        Body(value)
    }
}

#[derive(Clone, Debug, Error)]
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

/// Validate the header and body (when provided) of a single operation. All basic header
/// validation is performed (identical to [`validate_header`]()) and additionally the body bytes
/// hash and size are checked to be correct.
///
/// This method validates that the following conditions are true:
/// * Signature can be verified against the author public key and unsigned header bytes
/// * Header version is supported (currently only version 1 is supported)
/// * If `payload_hash` is set the `payload_size` is > `0` otherwise it is zero
/// * If `backlink` is set then `seq_num` is > `0` otherwise it is zero
/// * If provided the body bytes hash and size match those claimed in the header
pub fn validate_operation<E>(operation: &Operation<E>) -> Result<(), OperationError>
where
    E: Extensions,
{
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

/// Validate an operation header.
///
/// This method validates that the following conditions are true:
/// * Signature can be verified against the author public key and unsigned header bytes
/// * Header version is supported (currently only version 1 is supported)
/// * If `payload_hash` is set the `payload_size` is > `0` otherwise it is zero
/// * If `backlink` is set then `seq_num` is > `0` otherwise it is zero
pub fn validate_header<E>(header: &Header<E>) -> Result<(), OperationError>
where
    E: Extensions,
{
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

/// Validate a backlink contained in a header against a past header which is assumed to have been
/// retrieved from a local store.
///
/// This method validates that the following conditions are true:
/// * Current and past headers contain the same public key
/// * Current headers seq number increments from the past one by exactly `1`
/// * Backlink hash contained in the current header matches the hash of the past header
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
    use serde::{Deserialize, Serialize};

    use crate::{Extension, PrivateKey};

    use super::*;

    #[test]
    fn simple_extension_type_parameter() {
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
            extensions: (),
        };

        header.sign(&private_key);
    }

    #[test]
    fn sign_and_verify() {
        let private_key = PrivateKey::new();
        let body = Body::new("Hello, Sloth!".as_bytes());
        type CustomExtensions = ();

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
            extensions: None::<CustomExtensions>,
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
            extensions: (),
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
            extensions: (),
        };
        header_1.sign(&private_key);
        assert!(validate_header(&header_1).is_ok());

        assert!(validate_backlink(&header_0, &header_1).is_ok());
    }

    #[test]
    fn invalid_operations() {
        let private_key = PrivateKey::new();
        let body: Body = Body::new("Hello, Sloth!".as_bytes());

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
            extensions: (),
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

    #[test]
    fn extensions() {
        #[derive(Clone, Debug, Serialize, Deserialize)]
        struct LogId(Hash);

        #[derive(Clone, Debug, Serialize, Deserialize)]
        struct Expiry(u64);

        #[derive(Clone, Debug, Serialize, Deserialize)]
        struct CustomExtensions {
            log_id: Option<LogId>,
            expires: Expiry,
        }

        impl Extension<LogId> for CustomExtensions {
            fn extract(header: &Header<Self>) -> Option<LogId> {
                if header.seq_num == 0 {
                    return Some(LogId(header.hash()));
                };

                header.extensions.log_id.clone()
            }
        }

        impl Extension<Expiry> for CustomExtensions {
            fn extract(header: &Header<Self>) -> Option<Expiry> {
                Some(header.extensions.expires.clone())
            }
        }

        let extensions = CustomExtensions {
            log_id: None,
            expires: Expiry(0123456),
        };

        let private_key = PrivateKey::new();
        let body: Body = Body::new("Hello, Sloth!".as_bytes());

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
            extensions: extensions.clone(),
        };

        header.sign(&private_key);

        // Thanks to blanket implementation of Extension<T> on Header we can extract the extension
        // value from the header itself.
        let log_id: LogId = header.extension().unwrap();
        let expiry: Expiry = header.extension().unwrap();

        assert_eq!(header.hash(), log_id.0);
        assert_eq!(extensions.expires.0, expiry.0);
    }
}
