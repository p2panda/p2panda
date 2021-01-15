use crate::atomic::{Author, EntryEncoded, Hash, LogId, MessageEncoded, SeqNum};

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
    pub author: Author,

    /// Actual encoded Bamboo entry data.
    pub entry_encoded: EntryEncoded,

    /// Hash of Bamboo entry data.
    pub entry_hash: Hash,

    /// Used log for this entry.
    pub log_id: LogId,

    /// Encoded message payload of entry, can be deleted.
    pub message_encoded: Option<MessageEncoded>,

    /// Hash of message data.
    pub message_hash: Hash,

    /// Sequence number of this entry.
    pub seq_num: SeqNum,
}

impl Entry {}
