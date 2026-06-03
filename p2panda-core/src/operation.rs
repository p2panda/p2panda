// SPDX-License-Identifier: MIT OR Apache-2.0

//! Core p2panda data type offering distributed, secure and efficient data transfer between peers.
//!
//! Operations are used to carry any data from one peer to another (distributed), while assuming no
//! reliable network connection (offline-first) and untrusted machines (cryptographically secure).
//! The author of an operation uses it's [`SigningKey`] to cryptographically sign every operation.
//! This can be verified and used for authentication by any other peer.
//!
//! Every operation consists of a [`Header`] and an optional [`Body`]. The body holds arbitrary
//! bytes (up to the application to decide what should be inside). The header is used to
//! cryptographically secure & authenticate the body and for providing ordered collections of
//! operations when required.
//!
//! Operations have a `backlink` and `seq_num` field in the header. These are used to form a linked
//! list of operations, where every subsequent operation points to the previous one by referencing
//! its cryptographically secured hash.
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
//! use p2panda_core::{Body, Header, SigningKey};
//!
//! let signing_key = SigningKey::generate();
//!
//! let body = Body::from_bytes("Hello, Sloth!".as_bytes());
//! let mut header = Header {
//!     version: 1,
//!     verifying_key: signing_key.verifying_key(),
//!     signature: None,
//!     payload_size: body.size(),
//!     payload_hash: Some(body.hash()),
//!     seq_num: 0,
//!     backlink: None,
//!     extensions: (),
//! };
//!
//! header.sign(&signing_key);
//! ```
//!
//! ### Custom extensions
//!
//! ```
//! use p2panda_core::{Body, Extension, Header, SigningKey, PruneFlag};
//! use serde::{Serialize, Deserialize};
//!
//! let signing_key = SigningKey::generate();
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
//! let body = Body::from_bytes("Prune from here please!".as_bytes());
//! let mut header = Header {
//!     version: 1,
//!     verifying_key: signing_key.verifying_key(),
//!     signature: None,
//!     payload_size: body.size(),
//!     payload_hash: Some(body.hash()),
//!     seq_num: 0,
//!     backlink: None,
//!     extensions,
//! };
//!
//! header.sign(&signing_key);
//!
//! let prune_flag: PruneFlag = header.extension().unwrap();
//! assert!(prune_flag.is_set())
//! ```
use std::borrow::Borrow;

use thiserror::Error;

use crate::Extension;
use crate::extensions::Extensions;
use crate::hash::{HASH_LEN, Hash};
use crate::identity::{SIGNATURE_LEN, Signature, SigningKey, VERIFYING_KEY_LEN, VerifyingKey};
use crate::logs::SeqNum;
use crate::traits::{Chain, Digest, Offchain, Provenance};

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

impl<E> Borrow<Header<E>> for Operation<E> {
    fn borrow(&self) -> &Header<E> {
        &self.header
    }
}

#[allow(clippy::non_canonical_partial_ord_impl)]
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

impl<E> Digest<Hash> for Operation<E> {
    fn hash(&self) -> Hash {
        self.hash
    }
}

impl<E> Provenance<VerifyingKey> for Operation<E>
where
    E: Extensions,
{
    fn author(&self) -> VerifyingKey {
        self.header.verifying_key
    }

    fn verify(&self) -> bool {
        self.header.verify()
    }
}

impl<E> Chain<Hash> for Operation<E> {
    fn backlink(&self) -> Option<Hash> {
        self.header.backlink
    }

    fn seq_num(&self) -> SeqNum {
        self.header.seq_num
    }
}

impl<E> Offchain<Hash> for Operation<E> {
    fn payload(&self) -> Option<&Body> {
        self.body.as_ref()
    }

    fn payload_hash(&self) -> Option<Hash> {
        self.header.payload_hash
    }

    fn payload_size(&self) -> PayloadSize {
        self.header.payload_size
    }
}

#[derive(Clone, Debug)]
pub struct AnyOperation {
    pub hash: Hash,
    pub header: AnyHeader,
    pub body: Option<Body>,
}

impl Digest<Hash> for AnyOperation {
    fn hash(&self) -> Hash {
        self.hash
    }
}

impl Provenance<VerifyingKey> for AnyOperation {
    fn author(&self) -> VerifyingKey {
        self.header.verifying_key
    }

    fn verify(&self) -> bool {
        self.header.verify()
    }
}

impl Chain<Hash> for AnyOperation {
    fn backlink(&self) -> Option<Hash> {
        self.header.backlink
    }

    fn seq_num(&self) -> SeqNum {
        self.header.seq_num
    }
}

impl Offchain<Hash> for AnyOperation {
    fn payload(&self) -> Option<&Body> {
        self.body.as_ref()
    }

    fn payload_hash(&self) -> Option<Hash> {
        self.header.payload_hash
    }

    fn payload_size(&self) -> PayloadSize {
        self.header.payload_size
    }
}

impl TryFrom<RawOperation> for AnyOperation {
    type Error = AnyHeaderError;

    fn try_from(bytes: RawOperation) -> Result<Self, Self::Error> {
        let (header_bytes, body_bytes) = bytes;
        let header: AnyHeader = AnyHeader::decode(&header_bytes)?;

        Ok(AnyOperation {
            hash: header.hash(),
            header,
            body: body_bytes.map(Body::from),
        })
    }
}

impl<E> TryFrom<AnyOperation> for Operation<E>
where
    E: Extensions,
{
    type Error = AnyHeaderError;

    fn try_from(any_operation: AnyOperation) -> Result<Self, Self::Error> {
        let header: Header<E> = any_operation.header.try_into()?;
        Ok(Operation {
            header,
            body: any_operation.body,
            hash: any_operation.hash,
        })
    }
}

pub type Version = u16;

pub type PayloadSize = u32;

/// Header of a p2panda operation.
///
/// The header holds all metadata required to cryptographically secure and authenticate a message
/// [`Body`] and, if required, apply ordering to collections of messages from the same or many
/// authors.
///
/// ## Example
///
/// ```
/// use p2panda_core::{Body, Header, Operation, SigningKey};
///
/// let signing_key = SigningKey::generate();
///
/// let body = Body::from_bytes("Hello, Sloth!".as_bytes());
/// let mut header = Header {
///     version: 1,
///     verifying_key: signing_key.verifying_key(),
///     signature: None,
///     payload_size: body.size(),
///     payload_hash: Some(body.hash()),
///     seq_num: 0,
///     backlink: None,
///     extensions: (),
/// };
///
/// // Sign the header with the author's private key.
/// header.sign(&signing_key);
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub struct Header<E = ()> {
    /// Operation format version, allowing backwards compatibility when specification changes.
    pub version: Version,

    /// Author of this operation.
    pub verifying_key: VerifyingKey,

    /// Signature by author over all fields in header, providing authenticity.
    pub signature: Option<Signature>,

    /// Number of bytes of the body of this operation, must be zero if no body is given.
    pub payload_size: PayloadSize,

    /// Hash of the body of this operation, must be included if payload_size is non-zero and
    /// omitted otherwise.
    ///
    /// Keeping the hash here allows us to delete the payload (off-chain data) while retaining the
    /// ability to check the signature of the header.
    pub payload_hash: Option<Hash>,

    /// Number of operations this author has published to this log, begins with 0 and is always
    /// incremented by 1 with each new operation by the same author.
    pub seq_num: SeqNum,

    /// Hash of the previous operation of the same author and log. Can be omitted if first
    /// operation in log.
    pub backlink: Option<Hash>,

    /// Custom additional data.
    //
    // NOTE: If `E` is a Zero-Sized Type (ZST) we use unsafe code to skip the redundant field when
    // encoding or decoding the header. See `zero_sized_extensions` for safety details.
    //
    // This allows us to keep the usage of Header ergonomic while assuring operations are encoded
    // most efficiently and correctly according to p2panda's specification.
    //
    // An alternative would be to make this field an `Option` or introduce `E: Default` bounds to
    // allow initialisation in safe code which both are annoying to deal with.
    pub extensions: E,
}

impl<E: Default> Default for Header<E> {
    fn default() -> Self {
        Self {
            version: 1,
            verifying_key: VerifyingKey::default(),
            signature: None,
            payload_size: 0,
            payload_hash: None,
            seq_num: 0,
            backlink: None,
            extensions: E::default(),
        }
    }
}

impl<E> Header<E>
where
    E: Extensions,
{
    /// Header encoded to bytes in CBOR format.
    pub fn encode(&self) -> Vec<u8> {
        cbor_core::Value::serialized(&self)
            // We can be sure that all values in this module are serializable and _if_ the encoder
            // still fails then because of something really bad ..
            .expect("CBOR encoder failed due to an critical IO error")
            .encode()
    }

    /// Add a signature to the header using the provided `SigningKey`.
    ///
    /// This method signs the byte representation of a header with any existing signature removed
    /// before adding back the newly generated signature.
    pub fn sign(&mut self, signing_key: &SigningKey) {
        // Make sure the signature is not already set before we encode
        self.signature = None;

        let bytes = self.encode();
        self.signature = Some(signing_key.sign(&bytes));
    }

    /// BLAKE3 hash of the header bytes.
    ///
    /// This hash is used as the unique identifier of an operation, aka the Operation Id.
    pub fn hash(&self) -> Hash {
        Hash::digest(self.encode())
    }

    /// Size of header when encoded as CBOR bytes.
    pub fn size(&self) -> u32 {
        self.encode().len() as u32
    }

    /// Verify that the signature contained in this `Header` was generated by the claimed
    /// public key.
    pub fn verify(&self) -> bool {
        match self.signature {
            Some(claimed_signature) => {
                let mut unsigned_header = self.clone();
                unsigned_header.signature = None;
                let unsigned_bytes = unsigned_header.encode();
                self.verifying_key
                    .verify(&unsigned_bytes, &claimed_signature)
            }
            None => false,
        }
    }

    pub fn to_hex(&self) -> String {
        hex::encode(self.encode())
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
    pub(crate) const fn has_non_zero_sized_extensions() -> bool {
        std::mem::size_of::<E>() > 0
    }

    pub(crate) fn zero_sized_extensions() -> E {
        assert!(!Self::has_non_zero_sized_extensions());

        // SAFETY: The assertion guarantees E is a zero-sized type.
        //
        // For ZSTs, there are no bytes to initialize. std::mem::zeroed() on a ZST is a compile-time
        // no-op with no actual memory operations.
        unsafe { std::mem::zeroed() }
    }

    /// Number of fields included in the header.
    ///
    /// Fields instantiated with `None` values are excluded from the count.
    pub(crate) fn field_count(&self) -> usize {
        // There will always be a minimum of 4 fields in an unsigned header.
        let mut count = 4;

        if self.signature.is_some() {
            count += 1;
        }

        if self.payload_hash.is_some() {
            count += 1;
        }

        if self.backlink.is_some() {
            count += 1;
        }

        if Self::has_non_zero_sized_extensions() {
            count += 1;
        }

        count
    }
}

impl<E> TryFrom<Header<E>> for AnyHeader
where
    E: Extensions,
{
    type Error = AnyHeaderError;

    fn try_from(value: Header<E>) -> Result<Self, Self::Error> {
        AnyHeader::decode(&value.encode())
    }
}

impl<E> Digest<Hash> for Header<E>
where
    E: Extensions,
{
    fn hash(&self) -> Hash {
        self.hash()
    }
}

impl<E> Provenance<VerifyingKey> for Header<E>
where
    E: Extensions,
{
    fn author(&self) -> VerifyingKey {
        self.verifying_key
    }

    fn verify(&self) -> bool {
        self.verify()
    }
}

impl<E> Chain<Hash> for Header<E>
where
    E: Extensions,
{
    fn backlink(&self) -> Option<Hash> {
        self.backlink
    }

    fn seq_num(&self) -> SeqNum {
        self.seq_num
    }
}

impl<E> Offchain<Hash> for Header<E>
where
    E: Extensions,
{
    fn payload(&self) -> Option<&Body> {
        None // We don't have the body here.
    }

    fn payload_hash(&self) -> Option<Hash> {
        self.payload_hash
    }

    fn payload_size(&self) -> PayloadSize {
        self.payload_size
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AnyHeader {
    pub version: Version,
    pub verifying_key: VerifyingKey,
    pub signature: Signature,
    pub payload_size: PayloadSize,
    pub payload_hash: Option<Hash>,
    pub seq_num: SeqNum,
    pub backlink: Option<Hash>,
    size: u32,
    digest: Hash,
    extensions: Option<cbor_core::Value<'static>>,
}

impl AnyHeader {
    pub fn decode(bytes: &[u8]) -> Result<Self, AnyHeaderError> {
        // Attempt decoding bytes as CBOR.
        //
        // The bytes are decoded in a zero-copy manner, only reading from the given byte slice.
        let cbor = {
            let strict = cbor_core::DecodeOptions::new()
                // Reduce strictness as we can't enforce it for all possible ways users will encode
                // their extensions. On top we want to uphold the "robustness principle":
                //
                // > be conservative in what you do, be liberal in what you accept from others.
                .strictness(cbor_core::Strictness::LENIENT)
                // Still, we want to make sure some attacks are mitigated and set rather low
                // / pessimistic thresholds.
                .recursion_limit(64)
                .length_limit(64 * 1024)
                .oom_mitigation(64 * 1024);

            strict
                .decode(bytes)
                .map_err(AnyHeaderError::DecodingHeader)?
        };

        // Validate each field in header based on p2panda specification and extract Rust types.
        //
        // Every header is a tuple (CBOR array). We iterate over each field and check if the
        // expected CBOR and Rust type is given.
        //
        // The types are converted into owned objects (leaving the zero-copy nature of this process)
        // and kept to allow further validation (log integrity) or conversion into the more
        // specialised Header<E> type (where the Extensions are known).
        //
        // We don't keep the CBOR representation or bytes around anymore in the end (except of the
        // decoded extensions) to not waste memory with duplicate representations of the same data.
        let mut seq = cbor
            .into_array()
            .map_err(AnyHeaderError::UnexpectedHeaderType)?;
        let mut iter = seq.iter();

        let version = {
            let next = iter.next().ok_or(AnyHeaderError::MissingField("version"))?;

            Version::try_from(next)
                .map_err(|err| AnyHeaderError::UnexpectedFieldType(err, "version"))?
        };

        if version != 1 {
            return Err(AnyHeaderError::UnsupportedVersion(version, 1));
        }

        let verifying_key = {
            let next = iter
                .next()
                .ok_or(AnyHeaderError::MissingField("verifying_key"))?;

            let bytes = next
                .as_bytes()
                .map_err(|err| AnyHeaderError::UnexpectedFieldType(err, "verifying_key"))?;

            let bytes: [u8; VERIFYING_KEY_LEN] = bytes.try_into().map_err(|_| {
                AnyHeaderError::InvalidBytesLen("verifying_key", VERIFYING_KEY_LEN, bytes.len())
            })?;

            VerifyingKey::from_bytes(&bytes).map_err(AnyHeaderError::InvalidVerifyingKey)?
        };

        let signature = {
            let next = iter
                .next()
                .ok_or(AnyHeaderError::MissingField("signature"))?;

            let bytes = next
                .as_bytes()
                .map_err(|err| AnyHeaderError::UnexpectedFieldType(err, "signature"))?;

            let bytes: [u8; SIGNATURE_LEN] = bytes.try_into().map_err(|_| {
                AnyHeaderError::InvalidBytesLen("signature", SIGNATURE_LEN, bytes.len())
            })?;

            Signature::from(&bytes)
        };

        let payload_size = {
            let next = iter
                .next()
                .ok_or(AnyHeaderError::MissingField("payload_size"))?;

            PayloadSize::try_from(next)
                .map_err(|err| AnyHeaderError::UnexpectedFieldType(err, "payload_size"))?
        };

        let payload_hash = if payload_size > 0 {
            let next = iter
                .next()
                .ok_or(AnyHeaderError::MissingField("payload_hash"))?;

            let bytes = next
                .as_bytes()
                .map_err(|err| AnyHeaderError::UnexpectedFieldType(err, "payload_hash"))?;

            let bytes: [u8; HASH_LEN] = bytes.try_into().map_err(|_| {
                AnyHeaderError::InvalidBytesLen("payload_hash", HASH_LEN, bytes.len())
            })?;

            Some(Hash::from(bytes))
        } else {
            None
        };

        let seq_num = {
            let next = iter.next().ok_or(AnyHeaderError::MissingField("seq_num"))?;

            SeqNum::try_from(next)
                .map_err(|err| AnyHeaderError::UnexpectedFieldType(err, "seq_num"))?
        };

        let backlink = if seq_num > 0 {
            let next = iter
                .next()
                .ok_or(AnyHeaderError::MissingField("backlink"))?;

            let bytes = next
                .as_bytes()
                .map_err(|err| AnyHeaderError::UnexpectedFieldType(err, "backlink"))?;

            let bytes: [u8; HASH_LEN] = bytes
                .try_into()
                .map_err(|_| AnyHeaderError::InvalidBytesLen("backlink", HASH_LEN, bytes.len()))?;

            Some(Hash::from(bytes))
        } else {
            None
        };

        // Extract extensions and keep them for later, in case we need to deserialize them into
        // Header<E> in the future.
        //
        // AnyHeader doesn't know the Rust type for E, only it's "raw" CBOR representation. To use
        // extensions properly with Rust types we eventually want to convert into the concrete E
        // type.
        //
        // Please note that at this stage we _don't know_ if this header is valid with the
        // extensions set. We can only find out if this is correct if we know the concrete E type
        // (if it's a ZST then there should not be an extensions field).
        let extensions = iter.next().map(|value| value.to_owned());

        // If anything came after all expected fields, something is wrong.
        if iter.next().is_some() {
            return Err(AnyHeaderError::ExcessiveFields);
        }

        // Verify signature.
        //
        // Extract signature from field position 2. It'll be removed from the CBOR value, so we can
        // encode the bytes without it.
        //
        //  [0]      [1]            [2]
        // (version, verifying_key, signature, ..)
        //                          =========
        seq.remove(2);

        let verify_bytes = cbor_core::Value::from(seq).encode();
        if !verifying_key.verify(&verify_bytes, &signature) {
            return Err(AnyHeaderError::InvalidSignature);
        }

        // Calculate header size and generate hash digest.
        //
        // We keep these values around so if users of this object require the size or hash, it will
        // not be re-computed again.
        //
        // Since we also have the bytes in our hands already we don't need to encode either.
        let size = bytes.len() as u32;
        let digest = Hash::digest(bytes);

        Ok(Self {
            version,
            verifying_key,
            signature,
            payload_size,
            payload_hash,
            seq_num,
            backlink,
            size,
            digest,
            extensions,
        })
    }

    /// BLAKE3 hash of the header bytes.
    ///
    /// This hash is used as the unique identifier of an operation, aka the Operation Id.
    pub fn hash(&self) -> Hash {
        self.digest
    }

    /// Size of header when encoded as CBOR bytes.
    pub fn size(&self) -> u32 {
        self.size
    }
}

impl TryFrom<&[u8]> for AnyHeader {
    type Error = AnyHeaderError;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        Self::decode(value)
    }
}

impl TryFrom<Vec<u8>> for AnyHeader {
    type Error = AnyHeaderError;

    fn try_from(value: Vec<u8>) -> Result<Self, Self::Error> {
        Self::decode(&value)
    }
}

impl<E> TryFrom<AnyHeader> for Header<E>
where
    E: Extensions,
{
    type Error = AnyHeaderError;

    fn try_from(value: AnyHeader) -> Result<Self, Self::Error> {
        let extensions = match value.extensions {
            Some(cbor) => {
                // For ZST extension types we don't expect the extensions field in the header to be
                // set. Since we now know E we can assure that this is the case.
                if !Header::<E>::has_non_zero_sized_extensions() {
                    return Err(AnyHeaderError::UnexpectedExtensions);
                }

                // At this point we've already decoded the byte string into CBOR. Now we only need
                // serde to iterate over these values to check if they match the given Rust type.
                cbor.deserialized()
                    .map_err(AnyHeaderError::DecodingExtensions)?
            }
            None => {
                if Header::<E>::has_non_zero_sized_extensions() {
                    return Err(AnyHeaderError::MissingExtensions);
                } else {
                    Header::<E>::zero_sized_extensions()
                }
            }
        };

        Ok(Header {
            version: value.version,
            verifying_key: value.verifying_key,
            signature: Some(value.signature),
            payload_size: value.payload_size,
            payload_hash: value.payload_hash,
            seq_num: value.seq_num,
            backlink: value.backlink,
            extensions,
        })
    }
}

impl<E> TryFrom<(AnyHeader, Option<Body>)> for Operation<E>
where
    E: Extensions,
{
    type Error = AnyHeaderError;

    fn try_from(value: (AnyHeader, Option<Body>)) -> Result<Self, Self::Error> {
        let (any_header, body) = value;

        // Take the already computed hash from AnyHeader to save some time.
        let hash = any_header.hash();

        // Most fields have already been decoded, at this stage we only need to take the already
        // decoded CBOR values into a Rust type representation.
        let header: Header<E> = any_header.try_into()?;

        Ok(Operation { header, body, hash })
    }
}

impl Digest<Hash> for AnyHeader {
    fn hash(&self) -> Hash {
        self.hash()
    }
}

impl Provenance<VerifyingKey> for AnyHeader {
    fn author(&self) -> VerifyingKey {
        self.verifying_key
    }

    fn verify(&self) -> bool {
        // Was checked during decoding.
        true
    }
}

impl Chain<Hash> for AnyHeader {
    fn backlink(&self) -> Option<Hash> {
        self.backlink
    }

    fn seq_num(&self) -> SeqNum {
        self.seq_num
    }
}

impl Offchain<Hash> for AnyHeader {
    fn payload(&self) -> Option<&Body> {
        None
    }

    fn payload_hash(&self) -> Option<Hash> {
        self.payload_hash
    }

    fn payload_size(&self) -> PayloadSize {
        self.payload_size
    }
}

#[derive(Debug, Error)]
pub enum AnyHeaderError {
    #[error("failed decoding CBOR byte string for header: {0}")]
    DecodingHeader(cbor_core::Error),

    #[error("failed decoding CBOR byte string for extensions: {0}")]
    DecodingExtensions(cbor_core::SerdeError),

    #[error("expected CBOR array for header: {0}")]
    UnexpectedHeaderType(cbor_core::Error),

    #[error("missing \"{0}\" field in header")]
    MissingField(&'static str),

    #[error("unexpected \"{0}\" field type for \"{0}\"")]
    UnexpectedFieldType(cbor_core::Error, &'static str),

    #[error("invalid verifying key: {0}")]
    InvalidVerifyingKey(crate::identity::IdentityError),

    #[error("invalid bytes length for \"{0}\", expected {1}, got {2} bytes")]
    InvalidBytesLen(&'static str, usize, usize),

    #[error("operation version {0} is not supported, needs to be <= {1}")]
    UnsupportedVersion(Version, Version),

    #[error("invalid signature")]
    InvalidSignature,

    #[error("unexpected excessive fields in header")]
    ExcessiveFields,

    #[error("didn't expect extensions but header contained excessive field")]
    UnexpectedExtensions,

    #[error("expected extensions but header didn't contain any")]
    MissingExtensions,
}

/// Body of a p2panda operation containing arbitrary bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Body(pub(super) Vec<u8>);

impl Body {
    /// Construct a body from a byte slice.
    pub fn from_bytes(bytes: impl AsRef<[u8]>) -> Self {
        Self(bytes.as_ref().to_vec())
    }

    /// Access the underlying body bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        self.0.clone()
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// BLAKE3 hash of the body bytes.
    pub fn hash(&self) -> Hash {
        Hash::digest(&self.0)
    }

    /// Size of body bytes.
    pub fn size(&self) -> PayloadSize {
        self.0.len() as PayloadSize
    }

    pub fn to_hex(&self) -> String {
        hex::encode(self.as_bytes())
    }
}

impl From<&[u8]> for Body {
    fn from(value: &[u8]) -> Self {
        Body::from_bytes(value)
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
    UnsupportedVersion(Version, Version),

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
    SeqNumNonIncremental(SeqNum, SeqNum),

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
/// * If `payload_hash` is set the `payload_size` is > `0` otherwise it is zero
/// * If `backlink` is set then `seq_num` is > `0` otherwise it is zero
/// * If provided the body bytes hash and size match those claimed in the header
pub fn validate_operation<T>(operation: &T) -> Result<(), OperationError>
where
    T: Provenance<VerifyingKey> + Chain<Hash> + Offchain<Hash>,
{
    validate_header::<T>(operation)?;

    let claimed_payload_size = operation.payload_size();
    let claimed_payload_hash: Option<Hash> = match claimed_payload_size {
        0 => None,
        _ => {
            let hash = operation
                .payload_hash()
                .ok_or(OperationError::MissingPayloadHash)?;
            Some(hash)
        }
    };

    if let Some(body) = &operation.payload()
        && (claimed_payload_hash != Some(body.hash()) || claimed_payload_size != body.size())
    {
        return Err(OperationError::PayloadMismatch);
    }

    Ok(())
}

/// Validate an operation header.
///
/// This method validates that the following conditions are true:
/// * Signature can be verified against the author public key and unsigned header bytes
/// * If `payload_hash` is set the `payload_size` is > `0` otherwise it is zero
/// * If `backlink` is set then `seq_num` is > `0` otherwise it is zero
pub fn validate_header<T>(header: &T) -> Result<(), OperationError>
where
    T: Provenance<VerifyingKey> + Chain<Hash> + Offchain<Hash>,
{
    if !header.verify() {
        return Err(OperationError::SignatureMismatch);
    }

    if (header.payload_hash().is_some() && header.payload_size() == 0)
        || (header.payload_hash().is_none() && header.payload_size() > 0)
    {
        return Err(OperationError::InconsistentPayloadInfo);
    }

    if header.backlink().is_some() && header.seq_num() == 0 {
        return Err(OperationError::SeqNumMismatch);
    }

    if header.backlink().is_none() && header.seq_num() > 0 {
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
pub fn validate_backlink<T>(past_header: &T, header: &T) -> Result<(), OperationError>
where
    T: Provenance<VerifyingKey> + Digest<Hash> + Chain<Hash>,
{
    if past_header.author() != header.author() {
        return Err(OperationError::TooManyAuthors);
    }

    if past_header.seq_num() + 1 != header.seq_num() {
        return Err(OperationError::SeqNumNonIncremental(
            past_header.seq_num() + 1,
            header.seq_num(),
        ));
    }

    match header.backlink() {
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

    use crate::SigningKey;

    use super::*;

    #[test]
    fn zst_extension_type_parameter() {
        let signing_key = SigningKey::generate();
        let body = Body::from_bytes("Hello, Sloth!".as_bytes());
        let mut header = Header {
            version: 1,
            verifying_key: signing_key.verifying_key(),
            signature: None,
            payload_size: body.size(),
            payload_hash: Some(body.hash()),
            seq_num: 0,
            backlink: None,
            extensions: (),
        };

        header.sign(&signing_key);
    }

    #[test]
    fn sign_and_verify() {
        let signing_key = SigningKey::generate();
        let body = Body::from_bytes("Hello, Sloth!".as_bytes());

        type CustomExtensions = (u32, String);

        let mut header = Header::<CustomExtensions> {
            version: 1,
            verifying_key: signing_key.verifying_key(),
            signature: None,
            payload_size: body.size(),
            payload_hash: Some(body.hash()),
            seq_num: 0,
            backlink: None,
            extensions: (42, "penguin".to_string()),
        };
        assert!(!header.verify());

        header.sign(&signing_key);
        assert!(header.verify());

        let operation = Operation {
            hash: header.hash(),
            header,
            body: Some(body),
        };
        assert!(validate_operation::<Operation<_>>(&operation).is_ok());
    }

    #[test]
    fn valid_backlink_header() {
        let signing_key = SigningKey::generate();

        let mut header_0 = Header::<()> {
            version: 1,
            verifying_key: signing_key.verifying_key(),
            signature: None,
            payload_size: 0,
            payload_hash: None,
            seq_num: 0,
            backlink: None,
            extensions: (),
        };
        header_0.sign(&signing_key);
        assert!(validate_header(&header_0).is_ok());

        let mut header_1 = Header::<()> {
            version: 1,
            verifying_key: signing_key.verifying_key(),
            signature: None,
            payload_size: 0,
            payload_hash: None,
            seq_num: 1,
            backlink: Some(header_0.hash()),
            extensions: (),
        };
        header_1.sign(&signing_key);
        assert!(validate_header(&header_1).is_ok());

        assert!(validate_backlink(&header_0, &header_1).is_ok());
    }

    #[test]
    fn invalid_operations() {
        let signing_key = SigningKey::generate();
        let body: Body = Body::from_bytes("Hello, Sloth!".as_bytes());

        let header_base = Header::<()> {
            version: 1,
            verifying_key: signing_key.verifying_key(),
            signature: None,
            payload_size: body.size(),
            payload_hash: Some(body.hash()),
            seq_num: 0,
            backlink: None,
            extensions: (),
        };

        // Signature doesn't match public key
        let mut header = header_base.clone();
        header.verifying_key = SigningKey::generate().verifying_key();
        header.sign(&signing_key);
        assert!(matches!(
            validate_header(&header),
            Err(OperationError::SignatureMismatch)
        ));

        // Backlink missing
        let mut header = header_base.clone();
        header.seq_num = 1;
        header.sign(&signing_key);
        assert!(matches!(
            validate_header(&header),
            Err(OperationError::BacklinkMissing)
        ));

        // Backlink given but sequence number indicates none
        let mut header = header_base.clone();
        header.backlink = Some(Hash::digest(vec![4, 5, 6]));
        header.sign(&signing_key);
        assert!(matches!(
            validate_header(&header),
            Err(OperationError::SeqNumMismatch)
        ));

        // Payload size does not match
        let mut header = header_base.clone();
        header.payload_size = 11;
        header.sign(&signing_key);
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
        header.payload_hash = Some(Hash::digest(vec![4, 5, 6]));
        header.sign(&signing_key);
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
    fn zst_size_matches_mem_checks() {
        struct ZstExtensions;
        assert_eq!(std::mem::size_of::<ZstExtensions>(), 0);
        assert!(!Header::<ZstExtensions>::has_non_zero_sized_extensions());

        #[allow(unused)]
        struct NonZstExtensions(u32);
        assert_ne!(std::mem::size_of::<NonZstExtensions>(), 0);
        assert!(Header::<NonZstExtensions>::has_non_zero_sized_extensions());
    }

    #[test]
    fn any_header_conversions() {
        let signing_key = SigningKey::generate();

        #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
        struct TestExtensions {
            field_a: Vec<u8>,
            field_b: bool,
            field_c: u64,
        }

        let body = Body::from_bytes(b"hello");

        let mut header = Header::<TestExtensions> {
            version: 1,
            verifying_key: signing_key.verifying_key(),
            signature: None,
            payload_size: body.size(),
            payload_hash: Some(body.hash()),
            seq_num: 0,
            backlink: None,
            extensions: TestExtensions {
                field_a: vec![61, 112, 43],
                field_b: true,
                field_c: 54_938,
            },
        };
        header.sign(&signing_key);

        let hash = header.hash();
        assert!(header.verify());

        let any_header = AnyHeader::try_from(header.clone()).unwrap();
        assert_eq!(any_header.hash(), hash);
        assert_eq!(any_header.size(), header.encode().len() as u32);

        let header_again: Header<TestExtensions> = any_header.try_into().unwrap();
        assert_eq!(header, header_again);
    }

    #[test]
    fn any_header_errors() {
        let signing_key = SigningKey::generate();

        // payload size given without payload hash
        let mut header = Header::<()> {
            version: 1,
            verifying_key: signing_key.verifying_key(),
            signature: None,
            payload_size: 2829099,
            payload_hash: None,
            seq_num: 0,
            backlink: None,
            extensions: (),
        };
        header.sign(&signing_key);

        let result = AnyHeader::decode(&header.encode());
        assert!(result.is_err());

        // payload hash given without payload size
        let mut header = Header::<()> {
            version: 1,
            verifying_key: signing_key.verifying_key(),
            signature: None,
            payload_size: 0,
            payload_hash: Some(Hash::digest([0, 1, 2])),
            seq_num: 0,
            backlink: None,
            extensions: (),
        };
        header.sign(&signing_key);

        let result = AnyHeader::decode(&header.encode());
        assert!(result.is_err());

        // backlink given with seq number 0
        let mut header = Header::<()> {
            version: 1,
            verifying_key: signing_key.verifying_key(),
            signature: None,
            payload_size: 0,
            payload_hash: None,
            seq_num: 0,
            backlink: Some(Hash::digest([0, 1, 2])),
            extensions: (),
        };
        header.sign(&signing_key);

        // At this point we don't know that the backlink is _not_ an extension:
        let result = AnyHeader::decode(&header.encode()).expect("this is fine ..");

        // .. but latest here we'll find out!
        let result = Header::<()>::try_from(result);
        assert!(result.is_err());

        // backlink not given with seq number > 0
        let mut header = Header::<()> {
            version: 1,
            verifying_key: signing_key.verifying_key(),
            signature: None,
            payload_size: 0,
            payload_hash: None,
            seq_num: 10,
            backlink: None,
            extensions: (),
        };
        header.sign(&signing_key);

        let result = AnyHeader::decode(&header.encode());
        assert!(result.is_err());
    }
}
