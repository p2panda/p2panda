use crate::atomic::{
    Author, EntryEncoded, Hash, LogId, Message, MessageEncoded, SeqNum, Validation,
};
use crate::error::Result;
use crate::keypair::KeyPair;
use anyhow::bail;
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
    seq_num: Option<SeqNum>,
}

/// Error types for methods of `Entry` struct.
#[allow(missing_copy_implementations)]
#[derive(Error, Debug)]
pub enum EntryError {
    /// Invalid attempt to create an entry without a message.
    #[error("message fields can not be empty")]
    EmptyMessage,
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
            seq_num: previous_seq_num.map(|seq_num| seq_num.next().unwrap()),
        };
        entry.validate()?;
        Ok(entry)
    }

    pub fn backlink_hash(&self) -> Option<Hash> {
        if self.entry_hash_backlink.is_none() {
            return None;
        }

        Some(self.entry_hash_backlink.clone().unwrap())
    }

    pub fn encode(&self) -> EntryEncoded {
        self.entry_encoded.clone()
    }

    pub fn message(&self) -> Option<Message> {
        if self.message_encoded.is_none() {
            return None;
        }

        Some(self.message_encoded.clone().unwrap().decode())
    }

    pub fn message_encoded(&self) -> Option<MessageEncoded> {
        if self.message_encoded.is_none() {
            return None;
        }

        Some(self.message_encoded.clone().unwrap())
    }

    pub fn author(&self) -> Author {
        self.author.clone()
    }

    pub fn hash(&self) -> Hash {
        self.entry_hash.clone()
    }

    pub fn skiplink_hash(&self) -> Option<Hash> {
        if self.entry_hash_skiplink.is_none() {
            return None;
        }

        Some(self.entry_hash_skiplink.clone().unwrap())
    }

    pub fn message_hash(&self) -> Hash {
        self.message_hash.clone()
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
        // Create and update entries cannot have empty messages.
        if !self.has_message() || self.message().fields().unwrap().is_empty() {
            bail!(EntryError::EmptyMessage);
        }

        Ok(())
    }
}
