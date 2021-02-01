use crate::atomic::{
    Author, EntryEncoded, Hash, LogId, Message, MessageEncoded, SeqNum, Validation,
};
use crate::error::Result;
use crate::keypair::KeyPair;
use anyhow::bail;
use bamboo_rs_core::Entry as BambooEntry;
use thiserror::Error;

/// Entry of an append-only log based on Bamboo specification. It describes the actual data in the
/// p2p network and is shared between nodes.
///
/// Bamboo entries are the main data type of p2panda. Entries are organized in a distributed,
/// single-writer append-only log structure, created and signed by holders of private keys and
/// stored inside the node database.
///
/// The actual entry data is kept in `entry_encoded` and separated from the `message_encoded` as
/// the payload can be deleted without affecting the data structures integrity. All other fields
/// like `author`, `message_hash` etc. can be retrieved from `entry_encoded` but are separately
/// stored for easier access.
#[derive(Debug)]
pub struct Entry {
    /// Hash of previous Bamboo entry.
    entry_hash_backlink: Option<Hash>,

    /// Hash of skiplink Bamboo entry.
    entry_hash_skiplink: Option<Hash>,

    /// Used log for this entry.
    log_id: LogId,

    /// Encoded message payload of entry, can be deleted.
    message: Option<Message>,

    /// Sequence number of this entry.
    seq_num: SeqNum,
}

/// Error types for methods of `Entry` struct.
#[allow(missing_copy_implementations)]
#[derive(Error, Debug)]
pub enum EntryError {
    /// Invalid attempt to create an entry without a message.
    #[error("message fields can not be empty")]
    EmptyMessage,
    #[error("backlink and skiplink not valid for this sequence number")]
    InvalidLinks,
}

impl Entry {
    pub fn new(
        log_id: &LogId,
        message: &Message,
        entry_hash_skiplink: Option<&Hash>,
        entry_hash_backlink: Option<&Hash>,
        previous_seq_num: Option<&SeqNum>,
    ) -> Result<Self> {
        let entry = Self {
            log_id: log_id.clone().to_owned(),
            message: Some(message.clone().to_owned()),
            entry_hash_skiplink: entry_hash_skiplink.cloned(),
            entry_hash_backlink: entry_hash_backlink.cloned(),
            // If it is the first entry set sequence number to 1, otherwise increment.
            seq_num: previous_seq_num
                .map_or_else(|seq_num| seq_num.next().unwrap(), || SeqNum::default()),
        };
        entry.validate()?;
        Ok(entry)
    }

    pub fn sign_and_encode(&self, keypair: &KeyPair) -> EntryEncoded {
        let message_encoded = self.message.encode().unwrap();
        let message_hash = message_encoded.hash();

        let mut entry: BambooEntry<_, &[u8]> = BambooEntry {
            log_id: self.log_id.as_integer(),
            is_end_of_feed: false,
            payload_hash: message_hash,
            payload_size: message_encoded.size(),
            author: keypair.public,
            seq_num: self.seq_num.as_integer(),
            backlink: self.entry_hash_backlink.to_bytes(),
            lipmaa_link: self.entry_hash_skiplink.to_bytes(),
            sig: None,
        };
    }

    pub fn backlink_hash(&self) -> Option<Hash> {
        if self.entry_hash_backlink.is_none() {
            return None;
        }

        Some(self.entry_hash_backlink.clone().unwrap())
    }

    pub fn hash(&self) -> Hash {
        todo!();
    }

    pub fn skiplink_hash(&self) -> Option<Hash> {
        if self.entry_hash_skiplink.is_none() {
            return None;
        }

        Some(self.entry_hash_skiplink.clone().unwrap())
    }

    pub fn seq_num(&self) -> Option<SeqNum> {
        if self.seq_num.is_none() {
            return None;
        }

        Some(self.seq_num.clone().unwrap())
    }

    pub fn seq_num_backlink(&self) -> Option<SeqNum> {
        todo!();
    }

    pub fn seq_num_skiplink(&self) -> Option<SeqNum> {
        todo!();
    }

    /// Returns true if entry contains message.
    pub fn has_message(&self) -> bool {
        self.message.is_some()
    }
}

impl Validation for Entry {
    fn validate(&self) -> Result<()> {
        // First entries do not contain any sequence number or links. Every other entry has to contain all information.
        if (self.entry_hash_backlink.is_none()
            && self.entry_hash_skiplink.is_none()
            && self.seq_num.is_none())
            || (self.entry_hash_backlink.is_some()
                && self.entry_hash_skiplink.is_some()
                && self.seq_num.is_some())
        {
            bail!(EntryError::InvalidLinks);
        }

        Ok(())
    }
}
