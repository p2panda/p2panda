// SPDX-License-Identifier: AGPL-3.0-or-later

use bamboo_rs_core_ed25519_yasmf::entry::is_lipmaa_required;

use crate::entry::{LogId, SeqNum, Signature};
use crate::hash::Hash;
use crate::identity::Author;

pub trait AsEntry {
    /// Returns public key of entry.
    fn public_key(&self) -> &Author;

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

pub trait AsEncodedEntry {
    /// Generates and returns hash of encoded entry.
    fn hash(&self) -> Hash;

    /// Returns entry as bytes.
    fn into_bytes(&self) -> Vec<u8>;

    /// Returns payload size (number of bytes) of total encoded entry.
    fn size(&self) -> u64;
}
