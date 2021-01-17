use crate::atomic::{Author, EntryEncoded, Hash, LogId, Message, MessageEncoded, SeqNum};
use crate::error::Result;
use crate::keypair::KeyPair;

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
    /// Public key of the author.
    author: Author,

    /// Actual encoded Bamboo entry data.
    entry_encoded: EntryEncoded,

    /// Hash of Bamboo entry data.
    entry_hash: Hash,

    /// Hash of previous Bamboo entry.
    entry_hash_backlink: Option<Hash>,

    /// Hash of skiplink Bamboo entry.
    entry_hash_skiplink: Option<Hash>,

    /// Used log for this entry.
    log_id: LogId,

    /// Encoded message payload of entry, can be deleted.
    message_encoded: Option<MessageEncoded>,

    /// Hash of message data.
    message_hash: Hash,

    /// Sequence number of this entry.
    seq_num: Option<SeqNum>,
}

impl Entry {
    pub fn new(
        key_pair: &KeyPair,
        log_id: &LogId,
        message: &Message,
        entry_hash_skiplink: Option<&Hash>,
        entry_hash_backlink: Option<&Hash>,
        previous_seq_num: Option<&SeqNum>,
    ) -> Result<Self> {
        todo!();
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

    pub fn backlink_hash(&self) -> Option<Hash> {
        if self.entry_hash_backlink.is_none() {
            return None;
        }

        Some(self.entry_hash_backlink.clone().unwrap())
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
}
