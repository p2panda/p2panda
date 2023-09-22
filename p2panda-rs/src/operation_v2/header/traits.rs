// SPDX-License-Identifier: AGPL-3.0-or-later

//! Interfaces for interactions for entry-like structs.
use crate::entry::SIGNATURE_SIZE;
use crate::hash_v2::Hash;
use crate::identity_v2::{PublicKey, Signature};
use crate::operation_v2::header::{HeaderExtension, HeaderVersion};

/// Trait representing an "entry-like" struct.
pub trait AsHeader {
    fn version(&self) -> HeaderVersion;

    /// Returns public key of entry.
    fn public_key(&self) -> &PublicKey;

    /// Returns payload size of operation.
    fn payload_size(&self) -> u64;

    /// Returns payload hash of operation.
    fn payload_hash(&self) -> &Hash;

    /// Returns signature of entry.
    fn signature(&self) -> &Signature;

    /// Returns sequence number of entry.
    fn extensions(&self) -> &HeaderExtension;
}

/// Trait representing an "encoded entry-like" struct.
pub trait AsEncodedHeader {
    /// Generates and returns hash of encoded entry.
    fn hash(&self) -> Hash;

    /// Returns entry as bytes.
    ///
    /// TODO: Do we want to change this method naming?
    #[allow(clippy::wrong_self_convention)]
    fn into_bytes(&self) -> Vec<u8>;

    /// Returns payload size (number of bytes) of total encoded entry.
    fn size(&self) -> u64;

    /// Returns only those bytes of a signed entry that don't contain the signature.
    ///
    /// Encoded entries contains both a signature as well as the bytes that were signed. In order
    /// to verify the signature you need access to only the bytes that were used during signing.
    fn unsigned_bytes(&self) -> Vec<u8> {
        let bytes = self.into_bytes();
        let signature_offset = bytes.len() - SIGNATURE_SIZE;
        bytes[..signature_offset].into()
    }

    /// Returns the entry bytes encoded as a hex string.
    #[allow(clippy::wrong_self_convention)]
    fn into_hex(&self) -> String {
        hex::encode(self.into_bytes())
    }
}
