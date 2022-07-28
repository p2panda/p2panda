// SPDX-License-Identifier: AGPL-3.0-or-later

use std::convert::TryInto;
use std::hash::Hash as StdHash;

use bamboo_rs_core_ed25519_yasmf::entry::is_lipmaa_required;
use bamboo_rs_core_ed25519_yasmf::Entry as BambooEntry;

use crate::hash::Hash;
use crate::identity::{Author, KeyPair};
use crate::next::entry::encode::sign_entry;
use crate::next::entry::error::EntryBuilderError;
use crate::next::entry::{LogId, SeqNum, Signature};
use crate::next::operation::EncodedOperation;

/// Create and sign new `Entry` instances.
#[derive(Clone, Debug, Default)]
pub struct EntryBuilder {
    /// Used log for this entry.
    log_id: LogId,

    /// Sequence number of this entry.
    seq_num: SeqNum,

    /// Hash of skiplink Bamboo entry.
    skiplink: Option<Hash>,

    /// Hash of previous Bamboo entry.
    backlink: Option<Hash>,
}

impl EntryBuilder {
    /// Returns a new instance of `EntryBuilder`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set log id of entry.
    pub fn log_id(mut self, log_id: &LogId) -> Self {
        self.log_id = log_id.to_owned();
        self
    }

    /// Set sequence number of entry.
    pub fn seq_num(mut self, seq_num: &SeqNum) -> Self {
        self.seq_num = seq_num.to_owned();
        self
    }

    /// Set skiplink hash of entry.
    pub fn skiplink(mut self, hash: &Hash) -> Self {
        self.skiplink = Some(hash.to_owned());
        self
    }

    /// Set backlink hash of entry.
    pub fn backlink(mut self, hash: &Hash) -> Self {
        self.backlink = Some(hash.to_owned());
        self
    }

    /// Signs entry and secures payload with the author's key pair, returns a new `Entry` instance.
    ///
    /// An `EncodedOperation` is required here for the entry payload. The entry is "pointing" at
    /// the payload to secure and authenticate it. Later on, the payload can theoretically be
    /// deleted when it is not needed anymore.
    ///
    /// Using this method we can assume that the entry will be correctly signed. This applies only
    /// basic checks if the backlink and skiplink is correctly set for the given sequence number
    /// (#E3). Please note though that this method can not check for correct log integrity!
    pub fn sign(
        &self,
        encoded_operation: &EncodedOperation,
        key_pair: &KeyPair,
    ) -> Result<Entry, EntryBuilderError> {
        let entry = sign_entry(
            &self.log_id,
            &self.seq_num,
            self.skiplink.as_ref(),
            self.backlink.as_ref(),
            &encoded_operation,
            &key_pair,
        )?;

        Ok(entry)
    }
}

/// Entry of an append-only log based on [`Bamboo`] specification.
///
/// Bamboo entries are the main data type of p2panda. They describe the actual data in the p2p
/// network and are shared between nodes. Entries are organised in a distributed, single-writer
/// append-only log structure, created and signed by holders of private keys and stored inside the
/// node's database.
///
/// Entries are separated from the actual (off-chain) data to be able to delete application data
/// without loosing the integrity of the log. Payload data is formatted as "operations" in p2panda.
/// Each entry only holds a hash of the operation payload, this is why an [`Operation`] instance is
/// required during entry signing.
///
/// It is not possible to directly create an `Entry` instance without validation, use the
/// `EntryBuilder` to programmatically create and sign one or decode it from bytes via the
/// `EncodedEntry` struct.
///
/// [`Bamboo`]: https://github.com/AljoschaMeyer/bamboo
// @TODO: Fix pub(crate) visibility
#[derive(Debug, Clone, Eq, PartialEq, StdHash)]
pub struct Entry {
    /// Author of this entry.
    pub(crate) author: Author,

    /// Used log for this entry.
    pub(crate) log_id: LogId,

    /// Sequence number of this entry.
    pub(crate) seq_num: SeqNum,

    /// Hash of skiplink Bamboo entry.
    pub(crate) skiplink: Option<Hash>,

    /// Hash of previous Bamboo entry.
    pub(crate) backlink: Option<Hash>,

    /// Byte size of payload.
    pub(crate) payload_size: u64,

    /// Hash of payload.
    pub(crate) payload_hash: Hash,

    /// Ed25519 signature of entry.
    pub(crate) signature: Signature,
}

impl Entry {
    /// Returns public key of entry.
    pub fn public_key(&self) -> &Author {
        &self.author
    }

    /// Returns log id of entry.
    pub fn log_id(&self) -> &LogId {
        &self.log_id
    }

    /// Returns sequence number of entry.
    pub fn seq_num(&self) -> &SeqNum {
        &self.seq_num
    }

    /// Returns hash of skiplink entry when given.
    pub fn skiplink(&self) -> Option<&Hash> {
        self.skiplink.as_ref()
    }

    /// Returns hash of backlink entry when given.
    pub fn backlink(&self) -> Option<&Hash> {
        self.backlink.as_ref()
    }

    /// Returns payload size of operation.
    pub fn payload_size(&self) -> u64 {
        self.payload_size
    }

    /// Returns payload hash of operation.
    pub fn payload_hash(&self) -> &Hash {
        &self.payload_hash
    }

    /// Returns signature of entry.
    pub fn signature(&self) -> &Signature {
        &self.signature
    }

    /// Calculates sequence number of backlink entry.
    pub fn seq_num_backlink(&self) -> Option<SeqNum> {
        self.seq_num.backlink_seq_num()
    }

    /// Calculates sequence number of skiplink entry.
    pub fn seq_num_skiplink(&self) -> Option<SeqNum> {
        self.seq_num.skiplink_seq_num()
    }

    /// Returns true if skiplink has to be given.
    pub fn is_skiplink_required(&self) -> bool {
        is_lipmaa_required(self.seq_num.as_u64())
    }
}

impl From<BambooEntry<&[u8], &[u8]>> for Entry {
    fn from(entry: BambooEntry<&[u8], &[u8]>) -> Self {
        // Convert all hashes into our types
        let backlink: Option<Hash> = match entry.backlink {
            Some(link) => Some((&link).into()),
            None => None,
        };

        let skiplink: Option<Hash> = match entry.lipmaa_link {
            Some(link) => Some((&link).into()),
            None => None,
        };

        let payload_hash: Hash = (&entry.payload_hash).into();

        // Unwrap as we assume that there IS a signature coming from bamboo struct at this point
        let signature = entry.sig.expect("signature expected").into();

        // Unwrap as the sequence number was already checked when decoding the bytes into the
        // bamboo struct
        let seq_num = entry.seq_num.try_into().expect("invalid sequence number");

        Entry {
            author: (&entry.author).into(),
            log_id: entry.log_id.into(),
            seq_num,
            skiplink,
            backlink,
            payload_hash,
            payload_size: entry.payload_size,
            signature,
        }
    }
}