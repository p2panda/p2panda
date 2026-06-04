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
//! use p2panda_core::{Header, SigningKey};
//!
//! let signing_key = SigningKey::generate();
//!
//! let header = Header::builder()
//!     .body(b"Hello, Icebear!")
//!     .build(&signing_key, ());
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
//! let header = Header::builder()
//!     .body(&body)
//!     .build(&signing_key, extensions);
//!
//! let prune_flag: PruneFlag = header.extension().unwrap();
//! assert!(prune_flag.is_set())
//! ```
use std::borrow::Borrow;
use std::marker::PhantomData;

use cbor_core::Value;
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

pub struct Builder<E> {
    payload_size: PayloadSize,
    payload_hash: Option<Hash>,
    seq_num: SeqNum,
    backlink: Option<Hash>,
    _marker: PhantomData<E>,
}

impl<E> Default for Builder<E>
where
    E: Extensions,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<E> Builder<E>
where
    E: Extensions,
{
    pub fn new() -> Self {
        Self {
            payload_size: 0,
            payload_hash: None,
            seq_num: 0,
            backlink: None,
            _marker: PhantomData,
        }
    }

    pub fn body(mut self, bytes: impl AsRef<[u8]>) -> Self {
        let bytes = bytes.as_ref();

        self.payload_size = bytes.len() as PayloadSize;

        self.payload_hash = if self.payload_size == 0 {
            None
        } else {
            Some(Hash::digest(bytes))
        };

        self
    }

    pub fn chain(mut self, seq_num: SeqNum, backlink: Hash) -> Self {
        self.seq_num = seq_num;

        if self.seq_num > 0 {
            self.backlink = Some(backlink);
        } else {
            self.backlink = None;
        }

        self
    }

    pub fn build(self, signing_key: &SigningKey, extensions: E) -> Header<E> {
        let version = 1;

        let verifying_key = signing_key.verifying_key();

        let mut cbor = Value::array([
            Value::from(version),
            Value::from(verifying_key.as_bytes()),
            Value::from(self.payload_size),
        ]);

        if let Some(payload_hash) = &self.payload_hash {
            cbor.append(payload_hash.as_bytes());
        }

        cbor.append(self.seq_num);

        if let Some(backlink) = &self.backlink {
            cbor.append(backlink.as_bytes());
        }

        if Header::<E>::has_non_zero_sized_extensions() {
            cbor.append(cbor_core::Value::serialized(&extensions).unwrap());
        }

        let signing_bytes = cbor.encode();
        let signature = signing_key.sign(&signing_bytes);

        cbor.insert(2, signature.to_bytes());

        let bytes = cbor.encode();
        let digest = Hash::digest(&bytes);
        let size = bytes.len() as u32;

        Header {
            version,
            verifying_key,
            signature,
            payload_size: self.payload_size,
            payload_hash: self.payload_hash,
            seq_num: self.seq_num,
            backlink: self.backlink,
            extensions,
            size,
            digest,
        }
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
/// use p2panda_core::{Header, SigningKey};
///
/// let signing_key = SigningKey::generate();
///
/// let header = Header::builder()
///     .body(b"Hello, Icebear!")
///      // Sign the header with the author's private key.
///     .build(&signing_key, ());
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "arbitrary", derive(arbitrary::Arbitrary))]
pub struct Header<E = ()> {
    /// Operation format version, allowing backwards compatibility when specification changes.
    pub version: Version,

    /// Author of this operation.
    pub verifying_key: VerifyingKey,

    /// Signature by author over all fields in header, providing authenticity.
    pub signature: Signature,

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

    /// Size of header in encoded CBOR bytes.
    pub(crate) size: u32,

    /// BLAKE3 hash digest of header.
    pub(crate) digest: Hash,
}

#[cfg(any(test, feature = "test_utils"))]
impl<E> Default for Header<E>
where
    E: Default,
{
    // This is for hacky low-level access to this type, don't use this in production.
    fn default() -> Self {
        Self {
            version: 1,
            verifying_key: VerifyingKey::default(),
            signature: Signature::from([0; SIGNATURE_LEN]),
            payload_size: 0,
            payload_hash: None,
            seq_num: 0,
            backlink: None,
            extensions: E::default(),
            size: 0,
            digest: Hash::from([0; HASH_LEN]),
        }
    }
}

impl<E> Header<E>
where
    E: Extensions,
{
    pub fn builder() -> Builder<E> {
        Builder::new()
    }

    pub fn from_any(any_header: AnyHeader) -> Result<Self, AnyHeaderError> {
        Self::try_from(any_header)
    }

    pub fn encode(&self) -> Vec<u8> {
        self.encode_inner(false)
    }

    fn encode_inner(&self, skip_signature: bool) -> Vec<u8> {
        let mut cbor = Value::array([
            Value::from(self.version),
            Value::from(self.verifying_key.as_bytes()),
        ]);

        if !skip_signature {
            cbor.append(Value::from(self.signature.to_bytes()));
        }

        cbor.append(Value::from(self.payload_size));

        if let Some(payload_hash) = &self.payload_hash {
            cbor.append(payload_hash.as_bytes());
        }

        cbor.append(self.seq_num);

        if let Some(backlink) = &self.backlink {
            cbor.append(backlink.as_bytes());
        }

        if Header::<E>::has_non_zero_sized_extensions() {
            cbor.append(
                cbor_core::Value::serialized(&self.extensions)
                    .expect("serde and cbor_core to encode extensions"),
            );
        }

        cbor.encode()
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, AnyHeaderError> {
        // Decode header.
        let any_header = AnyHeader::decode(bytes)?;

        // Decode extensions.
        Self::from_any(any_header)
    }

    /// BLAKE3 hash of the header bytes.
    ///
    /// This hash is used as the unique identifier of an operation, aka the Operation Id.
    pub fn hash(&self) -> Hash {
        // Re-calculate hash and size in test environments.
        //
        // NOTE: This will lead to a different hash if trying to decode an unknown extensions format
        // since this re-encodes the extensions as well. Use `AnyHeader` for safe re-encoding in
        // test environments.
        if cfg!(any(test, feature = "test_utils")) {
            return Hash::digest(self.encode());
        }

        self.digest
    }

    /// Size of header when encoded as CBOR bytes.
    pub fn size(&self) -> u32 {
        // Re-calculate hash and size in test environments.
        if cfg!(any(test, feature = "test_utils")) {
            return self.encode().len() as u32;
        }

        self.size
    }

    #[cfg(any(test, feature = "test_utils"))]
    pub fn to_hex(&self) -> String {
        hex::encode(self.encode())
    }

    #[cfg(any(test, feature = "test_utils"))]
    pub fn sign(&mut self, signing_key: &SigningKey) {
        let signing_bytes = self.encode_inner(true);
        self.signature = signing_key.sign(&signing_bytes);
    }

    #[cfg(any(test, feature = "test_utils"))]
    pub fn verify(&self) -> bool {
        // NOTE: This will lead to an invalid signature if trying to decode an unknown extensions
        // format since this re-encodes the extensions as well. Use `AnyHeader` for safe re-encoding
        // in test environments.
        let signing_bytes = self.encode_inner(true);
        self.verifying_key.verify(&signing_bytes, &self.signature)
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
        // There will always be a minimum of 5 fields in an header.
        let mut count = 5;

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
        let extensions = if Header::<E>::has_non_zero_sized_extensions() {
            Some(
                cbor_core::Value::serialized(&value.extensions)
                    .map_err(AnyHeaderError::EncodingExtensions)?,
            )
        } else {
            None
        };

        Ok(AnyHeader {
            version: value.version,
            verifying_key: value.verifying_key,
            signature: value.signature,
            payload_size: value.payload_size,
            payload_hash: value.payload_hash,
            seq_num: value.seq_num,
            backlink: value.backlink,
            size: value.size,
            digest: value.digest,
            extensions,
        })
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
        // Check signature in test environments as low-level access might have allowed users to
        // tamper with the integrity.
        if cfg!(any(test, feature = "test_utils")) {
            return self.verify();
        }

        // Header was always created by us and has a valid signature.
        true
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

    pub fn encode(&self) -> Vec<u8> {
        let mut cbor = Value::array([
            Value::from(self.version),
            Value::from(self.verifying_key.as_bytes()),
            Value::from(self.signature.to_bytes()),
            Value::from(self.payload_size),
        ]);

        if let Some(payload_hash) = &self.payload_hash {
            cbor.append(payload_hash.as_bytes());
        }

        cbor.append(self.seq_num);

        if let Some(backlink) = &self.backlink {
            cbor.append(backlink.as_bytes());
        }

        if let Some(extensions) = &self.extensions {
            cbor.append(extensions.clone());
        }

        cbor.encode()
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
            signature: value.signature,
            payload_size: value.payload_size,
            payload_hash: value.payload_hash,
            seq_num: value.seq_num,
            backlink: value.backlink,
            extensions,
            size: value.size,
            digest: value.digest,
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

    #[error("failed encoding CBOR byte string for extensions: {0}")]
    EncodingExtensions(cbor_core::SerdeError),
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

    #[cfg(any(test, feature = "test_utils"))]
    pub fn to_hex(&self) -> String {
        hex::encode(&self.0)
    }
}

impl AsRef<[u8]> for Body {
    fn as_ref(&self) -> &[u8] {
        &self.0
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

    use crate::cbor::encode_cbor;
    use crate::identity::SigningKey;

    use super::*;

    #[test]
    fn paths_leading_to_same_encoding() {
        let signing_key = SigningKey::generate();

        let header = Header::builder()
            .body(b"test")
            .chain(2, Hash::from([2; 32]))
            .build(&signing_key, ());

        let hacky_header = {
            let body = Body::from_bytes(b"test");
            let mut hacky_header = Header::<()> {
                verifying_key: signing_key.verifying_key(),
                payload_size: body.size(),
                payload_hash: Some(body.hash()),
                seq_num: 2,
                backlink: Some(Hash::from([2; 32])),
                ..Default::default()
            };
            hacky_header.sign(&signing_key);
            hacky_header
        };

        assert_eq!(header.encode(), hacky_header.encode());
        assert_eq!(header.verify(), hacky_header.verify());
        assert!(header.verify());
        assert_eq!(header.hash(), hacky_header.hash());
        assert_eq!(header.size(), hacky_header.size());

        let any_header = AnyHeader::decode(&header.encode()).unwrap();

        assert_eq!(header.encode(), any_header.encode());
        assert_eq!(header.verify(), any_header.verify());
        assert!(any_header.verify());
        assert_eq!(header.hash(), any_header.hash());
        assert_eq!(header.size(), any_header.size());

        let header_again = Header::<()>::from_any(any_header.clone()).unwrap();
        assert_eq!(header, header_again);

        let serde_bytes = encode_cbor(&header).unwrap();
        assert_eq!(serde_bytes, header.encode());
    }

    #[test]
    fn sign_and_verify() {
        let signing_key = SigningKey::generate();
        let body = Body::from_bytes("Hello, Sloth!".as_bytes());

        type CustomExtensions = (u32, String);

        let header = Header::<CustomExtensions>::builder()
            .body(&body)
            .build(&signing_key, (42, "penguin".to_string()));
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

        let header_0 = Header::builder().build(&signing_key, ());
        assert!(validate_header(&header_0).is_ok());

        let header_1 = Header::builder()
            .chain(1, header_0.hash())
            .build(&signing_key, ());
        assert!(validate_header(&header_1).is_ok());

        assert!(validate_backlink(&header_0, &header_1).is_ok());
    }

    #[test]
    fn invalid_operations() {
        let signing_key = SigningKey::generate();
        let body: Body = Body::from_bytes("Hello, Sloth!".as_bytes());

        let header_base = Header::<()> {
            verifying_key: signing_key.verifying_key(),
            payload_size: body.size(),
            payload_hash: Some(body.hash()),
            ..Default::default()
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

        let header = Header::builder().body(b"hello").build(
            &signing_key,
            TestExtensions {
                field_a: vec![61, 112, 43],
                field_b: true,
                field_c: 54_938,
            },
        );

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

        // First check that this header is valid. The body is b"test".
        let correct_header_bytes = r#"[
            1,
            h'6dec6975e5c280e9eadde785cf01df20690d02c8ab7a57efcd58150ab53a867f',
            h'a4331f8d5742d3c40b7e8cfb93e487f4b3b8878e64f7ec9985169d6ed3a3fa3b7e9c9d4b69d6c62e6079dba734705a4b8fc2210f2793bf1ee8b2af5963ce9e0b',
            4,
            h'4878ca0425c739fa427f7eda20fe845f6b2e46ba5fe2a14df5b1e32f50603215',
            0
        ]"#
        .parse::<cbor_core::Value>()
        .unwrap()
        .encode();
        assert!(AnyHeader::decode(&correct_header_bytes).is_ok());

        // Insufficient bytes for payload_hash.
        let invalid_header_bytes = r#"[
            1,
            h'6dec6975e5c280e9eadde785cf01df20690d02c8ab7a57efcd58150ab53a867f',
            h'a4331f8d5742d3c40b7e8cfb93e487f4b3b8878e64f7ec9985169d6ed3a3fa3b7e9c9d4b69d6c62e6079dba734705a4b8fc2210f2793bf1ee8b2af5963ce9e0b',
            4,
            h'4878',
            0
        ]"#
        .parse::<cbor_core::Value>()
        .unwrap()
        .encode();

        std::assert_matches!(
            AnyHeader::decode(&invalid_header_bytes),
            Err(AnyHeaderError::InvalidBytesLen("payload_hash", 32, 2))
        );

        // Invalid signature.
        let mut header = Header::builder().build(&signing_key, ());
        header.verifying_key = SigningKey::generate().verifying_key();

        let result = AnyHeader::decode(&header.encode());
        std::assert_matches!(result, Err(AnyHeaderError::InvalidSignature));

        // payload_size given without payload_hash.
        let mut header = Header::builder().build(&signing_key, ());
        header.payload_size = 2829099;
        header.sign(&signing_key);

        let result = AnyHeader::decode(&header.encode());
        std::assert_matches!(
            result,
            Err(AnyHeaderError::UnexpectedFieldType(
                cbor_core::Error::IncompatibleType(cbor_core::DataType::Int),
                "payload_hash"
            ))
        );

        // payload_hash given without payload_size.
        let mut header = Header::<()> {
            verifying_key: signing_key.verifying_key(),
            payload_size: 0,
            payload_hash: Some(Hash::digest([0, 1, 2])),
            extensions: (),
            ..Default::default()
        };
        header.sign(&signing_key);

        let result = AnyHeader::decode(&header.encode());
        std::assert_matches!(
            result,
            Err(AnyHeaderError::UnexpectedFieldType(
                cbor_core::Error::IncompatibleType(cbor_core::DataType::Bytes),
                "seq_num"
            ))
        );

        // backlink given with seq_num 0.
        let mut header = Header::<()> {
            verifying_key: signing_key.verifying_key(),
            seq_num: 0,
            backlink: Some(Hash::digest([0, 1, 2])),
            ..Default::default()
        };
        header.sign(&signing_key);

        // At this point we don't know that the backlink is _not_ an extension:
        let result = AnyHeader::decode(&header.encode()).expect("this is fine ..");

        // .. but latest here we'll find out!
        let result = Header::<()>::try_from(result);
        let result_2 = Header::<()>::decode(&header.encode());
        assert!(result.is_err());
        assert!(result_2.is_err());
        std::assert_matches!(result, Err(AnyHeaderError::UnexpectedExtensions));

        // backlink not given with seq_num > 0.
        let mut header = Header::<()> {
            verifying_key: signing_key.verifying_key(),
            seq_num: 10,
            backlink: None,
            ..Default::default()
        };
        header.sign(&signing_key);

        let result = AnyHeader::decode(&header.encode());
        std::assert_matches!(result, Err(AnyHeaderError::MissingField("backlink")));
    }

    #[test]
    fn forwards_compatible_checks() {
        use crate::{PruneFlag, Timestamp};

        #[derive(Clone, Debug, Serialize, Deserialize)]
        struct LegacyExtensionsFormat {
            timestamp: Timestamp,
        }

        #[derive(Clone, Debug, Serialize, Deserialize)]
        struct FutureExtensionsFormat {
            timestamp: Timestamp,
            #[serde(default = "PruneFlag::default")]
            prune_flag: PruneFlag,
        }

        let signing_key = SigningKey::generate();

        let old_header = Header::builder().body(b"once upon a time").build(
            &signing_key,
            LegacyExtensionsFormat {
                timestamp: 1780572316919.into(),
            },
        );
        let old_header_bytes = old_header.encode();

        let new_header = Header::builder()
            .body(b"fitter, happier, more productive")
            .build(
                &signing_key,
                FutureExtensionsFormat {
                    timestamp: 1780572316919.into(),
                    prune_flag: true.into(),
                },
            );
        let new_header_bytes = new_header.encode();
        let new_header_hash = new_header.hash();

        // The old system can still parse headers with the new extensions format:
        let any_header = AnyHeader::decode(&new_header_bytes).unwrap();

        // The signature was checked during decoding already.
        assert!(any_header.verify());

        // .. and the hash digest matches with the original even though we don't know the new
        // extension format:
        assert_eq!(new_header_hash, any_header.hash());

        // It can even parse the extensions, will omit the unknown prune_flag field.
        let header = Header::<LegacyExtensionsFormat>::from_any(any_header).unwrap();
        assert_eq!(header.extensions.timestamp, 1780572316919.into());

        // The old system can still parse headers with the new extensions format, not too important
        // for this test, but nice to show:
        let header = Header::<FutureExtensionsFormat>::decode(&old_header_bytes).unwrap();
        assert_eq!(header.extensions.timestamp, 1780572316919.into());
        assert_eq!(header.extensions.prune_flag, false.into()); // set to default when not given
    }
}
