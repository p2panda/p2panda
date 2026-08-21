// SPDX-License-Identifier: MIT OR Apache-2.0

use cbor_core::Value;

use crate::hash::Hash;
#[cfg(any(test, feature = "test_utils"))]
use crate::identity::SigningKey;
use crate::identity::{Signature, VerifyingKey};
use crate::logs::SeqNum;
use crate::operation::{AnyHeader, Builder};
use crate::traits::{Chain, Digest, Extensions, Offchain, Provenance};
use crate::{Body, HeaderError};

/// Operation format version.
pub type Version = u16;

/// Number of bytes of the body of this operation.
pub type PayloadSize = u32;

/// Header of a p2panda operation with known extensions type.
///
/// The header holds all metadata required to cryptographically secure and authenticate a message
/// [`Body`] and it's custom extensions.
///
/// See [`AnyHeader`] for dealing with headers when you don't care about the concrete extensions
/// type (`E`).
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

    /// Original extensions representation in CBOR AST.
    ///
    /// This allows us to correctly re-encode this header to bytes if necessary. If we would encode
    /// the extensions from the Rust type `E` we might not be able to re-construct the original
    /// bytes. A different system might have interpreted the extensions differently.
    pub(crate) extensions_cbor: Option<cbor_core::Value<'static>>,

    /// Size of header in encoded CBOR bytes.
    pub(crate) size: u32,

    /// BLAKE3 hash digest of header.
    pub(crate) digest: Hash,
}

impl<E> Header<E>
where
    E: Extensions,
{
    /// Returns builder to create & sign new header.
    pub fn builder() -> Builder<E> {
        Builder::new()
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
            self.extensions_cbor.as_ref(),
        )
    }

    /// Attempts decoding header from bytes.
    ///
    /// This might fail if integrity checks failed or header formatting is invalid.
    pub fn decode(bytes: &[u8]) -> Result<Self, HeaderError> {
        // Decode header.
        let any_header = AnyHeader::decode(bytes)?;

        // Decode extensions.
        Self::try_from(any_header)
    }

    /// BLAKE3 hash digest of the header bytes.
    ///
    /// This hash is used as the unique identifier of an operation, aka the Operation Id.
    pub fn hash(&self) -> Hash {
        // Re-calculate hash and size in test environments.
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
}

impl<E> Header<E> {
    pub(crate) const fn has_zero_sized_extensions() -> bool {
        std::mem::size_of::<E>() == 0
    }

    pub(crate) fn zero_sized_extensions() -> E {
        assert!(Self::has_zero_sized_extensions());

        // SAFETY: The assertion guarantees E is a zero-sized type.
        //
        // For ZSTs, there are no bytes to initialize. std::mem::zeroed() on a ZST is a compile-time
        // no-op with no actual memory operations.
        unsafe { std::mem::zeroed() }
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

#[allow(clippy::too_many_arguments)]
pub(crate) fn encode_header(
    version: Version,
    verifying_key: VerifyingKey,
    signature: Option<&Signature>,
    payload_size: PayloadSize,
    payload_hash: Option<Hash>,
    seq_num: SeqNum,
    backlink: Option<Hash>,
    extensions: Option<&Value<'static>>,
) -> Vec<u8> {
    let mut cbor = Value::array([Value::from(version), Value::from(verifying_key.as_bytes())]);

    // Signature can be omitted to encode bytes for signing.
    if let Some(signature) = &signature {
        cbor.append(signature.to_bytes());
    }

    cbor.append(payload_size);

    if let Some(payload_hash) = &payload_hash {
        cbor.append(payload_hash.as_bytes());
    }

    cbor.append(seq_num);

    if let Some(backlink) = &backlink {
        cbor.append(backlink.as_bytes());
    }

    // We're serializing from the AST using cbor_core. If decoding an extension from another
    // code-base (which was generated using another CBOR encoder with different rules) and encoding
    // it here again, we might end up with a different byte sequence and thus hash digest.
    //
    // This can for example happen if the given extension uses non-canonical CBOR encoding,
    // ambigious map ordering etc.
    //
    // To mitigate this from happening we're enforcing a strict, canonical CBOR encoding when
    // decoding the extensions bytes.
    if let Some(extensions) = extensions {
        cbor.append(extensions.to_owned());
    }

    cbor.encode()
}

impl<E> TryFrom<AnyHeader> for Header<E>
where
    E: Extensions,
{
    type Error = HeaderError;

    fn try_from(value: AnyHeader) -> Result<Self, Self::Error> {
        let extensions = match value.extensions {
            Some(ref cbor) => {
                // For ZST extension types we don't expect the extensions field in the header to be
                // set. Since we now know E we can assure that this is the case.
                if Header::<E>::has_zero_sized_extensions() {
                    return Err(HeaderError::UnexpectedExtensions);
                }

                // At this point we've already decoded the byte string into CBOR. Now we only need
                // serde to iterate over these values to check if they match the given Rust type.
                cbor.deserialized()
                    .map_err(HeaderError::DecodingExtensions)?
            }
            None => {
                if !Header::<E>::has_zero_sized_extensions() {
                    return Err(HeaderError::MissingExtensions);
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
            extensions_cbor: value.extensions,
            size: value.size,
            digest: value.digest,
        })
    }
}

#[cfg(any(test, feature = "test_utils"))]
impl<E> Default for Header<E>
where
    E: Default,
{
    /// This is for hacky low-level access to this type, don't use this in production.
    ///
    /// Size and digest get re-computed whenever called in test environments. Note that we can't
    /// re-encode `extensions_cbor` if `E` was changed in a test. Ideally you don't want to test
    /// extensions-related code here anyway.
    fn default() -> Self {
        use crate::hash::HASH_LEN;
        use crate::identity::SIGNATURE_LEN;

        Self {
            version: 1,
            verifying_key: VerifyingKey::default(),
            signature: Signature::from([0; SIGNATURE_LEN]),
            payload_size: 0,
            payload_hash: None,
            seq_num: 0,
            backlink: None,
            extensions: E::default(),
            extensions_cbor: None,
            size: 0,
            digest: Hash::from([0; HASH_LEN]),
        }
    }
}

#[cfg(any(test, feature = "test_utils"))]
impl<E> Header<E>
where
    E: Extensions,
{
    pub fn to_hex(&self) -> String {
        hex::encode(self.encode())
    }

    fn encode_signing_bytes(&self) -> Vec<u8> {
        encode_header(
            self.version,
            self.verifying_key,
            None,
            self.payload_size,
            self.payload_hash,
            self.seq_num,
            self.backlink,
            self.extensions_cbor.as_ref(),
        )
    }

    pub fn sign(&mut self, signer: &SigningKey) {
        let signing_bytes = self.encode_signing_bytes();
        self.signature = signer.sign(&signing_bytes);
        self.update_size_and_digest();
    }

    pub fn verify(&self) -> bool {
        let signing_bytes = self.encode_signing_bytes();
        self.verifying_key.verify(&signing_bytes, &self.signature)
    }

    fn update_size_and_digest(&mut self) {
        self.size = self.size();
        self.digest = self.hash();
    }
}

#[cfg(feature = "arbitrary")]
impl<'a, E> arbitrary::Arbitrary<'a> for Header<E>
where
    E: Default + Extensions,
{
    fn arbitrary(u: &mut arbitrary::Unstructured<'a>) -> arbitrary::Result<Self> {
        use crate::hash::HASH_LEN;
        use crate::identity::SIGNATURE_LEN;

        let header = Header {
            version: 1,
            verifying_key: u.arbitrary()?,
            signature: Signature::from_bytes(&[0; SIGNATURE_LEN]),
            payload_size: u.arbitrary()?,
            payload_hash: u.arbitrary()?,
            seq_num: u.arbitrary()?,
            backlink: u.arbitrary()?,
            extensions: E::default(),
            extensions_cbor: None,
            size: 0,
            digest: Hash::from_bytes([0; HASH_LEN]),
        };

        Ok(header)
    }
}

#[cfg(test)]
mod tests {
    use super::Header;

    #[test]
    fn zst_size_matches_mem_checks() {
        struct ZstExtensions;
        assert_eq!(std::mem::size_of::<ZstExtensions>(), 0);
        assert!(Header::<ZstExtensions>::has_zero_sized_extensions());

        #[allow(unused)]
        struct NonZstExtensions(u32);
        assert_ne!(std::mem::size_of::<NonZstExtensions>(), 0);
        assert!(!Header::<NonZstExtensions>::has_zero_sized_extensions());
    }
}
