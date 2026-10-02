// SPDX-License-Identifier: MIT OR Apache-2.0

use crate::Operation;
use crate::hash::{HASH_LEN, Hash};
use crate::identity::{SIGNATURE_LEN, Signature, VERIFYING_KEY_LEN, VerifyingKey};
use crate::logs::SeqNum;
use crate::operation::HeaderError;
use crate::operation::header::encode_header;
use crate::operation::{Body, Header, PayloadSize, RawOperation, Version};
use crate::traits::{Chain, Digest, Offchain, Provenance};

/// Combined [`AnyHeader`], [`Body`] and operation [`struct@Hash`] (Operation Id).
///
/// ## Extensions
///
/// `AnyOperation` does not know the concrete extensions type. On this level it is only concerned
/// with the validity and integrity of the append-only log type itself which is enough for most
/// low-level protocols, such as the sync protocol.
///
/// Applications usually want to attach custom extensions to the operation, if you need to know the
/// type you can easily convert from `AnyOperation` to [`Operation`](crate::Operation) with an
/// explicit `E` extensions type.
#[derive(Clone, Debug, PartialEq)]
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
    type Error = HeaderError;

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

impl<E> From<Operation<E>> for AnyOperation {
    fn from(value: Operation<E>) -> Self {
        AnyOperation {
            hash: value.hash,
            header: value.header.into(),
            body: value.body,
        }
    }
}

/// Header of a p2panda operation.
///
/// The header holds all metadata required to cryptographically secure and authenticate a message
/// [`Body`] and it's custom extensions.
///
/// ## Extensions
///
/// `AnyHeader` does not know the concrete extensions type. On this level it is only concerned with
/// the validity and integrity of the append-only log type itself which is enough for most low-level
/// protocols, such as the sync protocol.
///
/// Applications usually want to attach custom extensions to the header, if you need to know the
/// type you can easily convert from `AnyHeader` to [`Header`] with an explicit `E` extensions
/// type.
///
/// ```rust
/// # fn example() -> Result<(), p2panda_core::HeaderError> {
/// use p2panda_core::{AnyHeader, Hash, Header, SigningKey};
/// use serde::{Deserialize, Serialize};
///
/// #[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
/// struct MyExtensions {
///     dependencies: Vec<Hash>,
/// }
///
/// let signing_key = SigningKey::generate();
///
/// // Create a Header with concrete extension type `MyExtensions`.
/// let header = Header::builder()
///     .build(&signing_key, MyExtensions {
///         dependencies: vec![Hash::from([0; 32])],
///     });
///
/// // Encode it to CBOR bytes, this is how we transmit operations over the network.
/// let bytes = header.encode();
///
/// // Convert it to `AnyHeader` which doesn't know the extensions type.
/// let any_header = AnyHeader::decode(&bytes)?;
///
/// // Bring it back to a concrete Header type with `MyExtensions`.
/// let header_again = Header::try_from(any_header)?;
/// assert_eq!(header, header_again);
/// # Ok(())
/// # }
/// ```
///
/// Please note that at this stage we can only verify the integrity and authenticity of the attached
/// extensions, we _don't know_ if the extensions themselves are valid. We can only find out if this
/// is correct if we know the concrete E type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AnyHeader {
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

    /// Size of header in encoded CBOR bytes.
    pub(crate) size: u32,

    /// BLAKE3 hash digest of header.
    pub(crate) digest: Hash,

    /// Custom additional data.
    ///
    /// We don't know the exact Rust type of the extensions here, only the AST representation of
    /// CBOR. To decode the value to an extensions Rust type `E` use
    /// `Header::<E>::try_from`(crate::Header::try_from).
    pub(crate) extensions: Option<cbor_core::Value<'static>>,
}

impl AnyHeader {
    /// Attempts decoding header from bytes.
    ///
    /// This fails if integrity checks failed or header formatting is invalid.
    pub fn decode(bytes: &[u8]) -> Result<Self, HeaderError> {
        // Attempt decoding bytes as CBOR.
        //
        // The bytes are decoded in a zero-copy manner, only reading from the given byte slice.
        let cbor = {
            let codec = cbor_core::DecodeOptions::new()
                // Enforce a strict, canonical CBOR encoding, otherwise integrity checks would fail
                // when decoding & encoding the headers again on our end. See `encode_header` for
                // details.
                .strictness(cbor_core::Strictness::STRICT)
                // Make sure some attacks are mitigated and set rather low / pessimistic thresholds.
                .recursion_limit(64)
                .length_limit(512) // 0.5kb
                .oom_mitigation(64);

            codec.decode(bytes).map_err(HeaderError::DecodingHeader)?
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
            .map_err(HeaderError::UnexpectedHeaderType)?;
        let mut iter = seq.iter();

        let version = {
            let next = iter.next().ok_or(HeaderError::MissingField("version"))?;

            Version::try_from(next)
                .map_err(|err| HeaderError::UnexpectedFieldType(err, "version"))?
        };

        if version != 1 {
            return Err(HeaderError::UnsupportedVersion(version, 1));
        }

        let verifying_key = {
            let next = iter
                .next()
                .ok_or(HeaderError::MissingField("verifying_key"))?;

            let bytes = next
                .as_bytes()
                .map_err(|err| HeaderError::UnexpectedFieldType(err, "verifying_key"))?;

            let bytes: [u8; VERIFYING_KEY_LEN] = bytes.try_into().map_err(|_| {
                HeaderError::InvalidBytesLen("verifying_key", VERIFYING_KEY_LEN, bytes.len())
            })?;

            VerifyingKey::from_bytes(&bytes).map_err(HeaderError::InvalidVerifyingKey)?
        };

        let signature = {
            let next = iter.next().ok_or(HeaderError::MissingField("signature"))?;

            let bytes = next
                .as_bytes()
                .map_err(|err| HeaderError::UnexpectedFieldType(err, "signature"))?;

            let bytes: [u8; SIGNATURE_LEN] = bytes.try_into().map_err(|_| {
                HeaderError::InvalidBytesLen("signature", SIGNATURE_LEN, bytes.len())
            })?;

            Signature::from(&bytes)
        };

        let payload_size = {
            let next = iter
                .next()
                .ok_or(HeaderError::MissingField("payload_size"))?;

            PayloadSize::try_from(next)
                .map_err(|err| HeaderError::UnexpectedFieldType(err, "payload_size"))?
        };

        let payload_hash = if payload_size > 0 {
            let next = iter
                .next()
                .ok_or(HeaderError::MissingField("payload_hash"))?;

            let bytes = next
                .as_bytes()
                .map_err(|err| HeaderError::UnexpectedFieldType(err, "payload_hash"))?;

            let bytes: [u8; HASH_LEN] = bytes
                .try_into()
                .map_err(|_| HeaderError::InvalidBytesLen("payload_hash", HASH_LEN, bytes.len()))?;

            Some(Hash::from(bytes))
        } else {
            None
        };

        let seq_num = {
            let next = iter.next().ok_or(HeaderError::MissingField("seq_num"))?;

            SeqNum::try_from(next)
                .map_err(|err| HeaderError::UnexpectedFieldType(err, "seq_num"))?
        };

        let backlink = if seq_num > 0 {
            let next = iter.next().ok_or(HeaderError::MissingField("backlink"))?;

            let bytes = next
                .as_bytes()
                .map_err(|err| HeaderError::UnexpectedFieldType(err, "backlink"))?;

            let bytes: [u8; HASH_LEN] = bytes
                .try_into()
                .map_err(|_| HeaderError::InvalidBytesLen("backlink", HASH_LEN, bytes.len()))?;

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
            return Err(HeaderError::ExcessiveFields);
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
            return Err(HeaderError::InvalidSignature);
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

    /// Encodes header to byte-representation (CBOR).
    pub fn encode(&self) -> Vec<u8> {
        encode_header(
            self.version,
            self.verifying_key,
            Some(&self.signature),
            self.payload_size,
            self.payload_hash,
            self.seq_num,
            self.backlink,
            self.extensions.as_ref(),
        )
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

impl TryFrom<&[u8]> for AnyHeader {
    type Error = HeaderError;

    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        Self::decode(value)
    }
}

impl TryFrom<Vec<u8>> for AnyHeader {
    type Error = HeaderError;

    fn try_from(value: Vec<u8>) -> Result<Self, Self::Error> {
        Self::decode(&value)
    }
}

impl<E> From<Header<E>> for AnyHeader {
    fn from(value: Header<E>) -> Self {
        AnyHeader {
            version: value.version,
            verifying_key: value.verifying_key,
            signature: value.signature,
            payload_size: value.payload_size,
            payload_hash: value.payload_hash,
            seq_num: value.seq_num,
            backlink: value.backlink,
            size: value.size,
            digest: value.digest,
            extensions: value.extensions_cbor,
        }
    }
}
