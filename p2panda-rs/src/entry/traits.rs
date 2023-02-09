// SPDX-License-Identifier: AGPL-3.0-or-later

//! Interfaces for interactions for entry-like structs.
use bamboo_rs_core_ed25519_yasmf::entry::is_lipmaa_required;

use crate::entry::SIGNATURE_SIZE;
use crate::entry::{LogId, SeqNum, Signature};
use crate::hash::Hash;
use crate::identity::PublicKey;

/// Trait representing an "entry-like" struct.
pub trait AsEntry {
    /// Returns public key of entry.
    fn public_key(&self) -> &PublicKey;

    /// Returns log id of entry.
    fn log_id(&self) -> &LogId;

    /// Returns sequence number of entry.
    fn seq_num(&self) -> &SeqNum;

    /// Returns hash of skiplink entry when given.
    fn skiplink(&self) -> Option<&Hash>;

    /// Returns hash of backlink entry when given.
    fn backlink(&self) -> Option<&Hash>;

    /// Returns payload size of operation.
    fn payload_size(&self) -> u64;

    /// Returns payload hash of operation.
    fn payload_hash(&self) -> &Hash;

    /// Returns signature of entry.
    fn signature(&self) -> &Signature;

    /// Calculates sequence number of backlink entry.
    fn seq_num_backlink(&self) -> Option<SeqNum> {
        self.seq_num().backlink_seq_num()
    }

    /// Calculates sequence number of skiplink entry.
    fn seq_num_skiplink(&self) -> Option<SeqNum> {
        self.seq_num().skiplink_seq_num()
    }

    /// Returns true if skiplink has to be given.
    fn is_skiplink_required(&self) -> bool {
        is_lipmaa_required(self.seq_num().as_u64())
    }
}

/// Trait representing an "encoded entry-like" struct.
pub trait AsEncodedEntry {
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
