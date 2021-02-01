use anyhow::bail;
use bamboo_rs_core::{Entry as BambooEntry, YamfHash};
use thiserror::Error;

use crate::atomic::{EntryEncoded, Hash, LogId, Message, SeqNum, Validation};
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
    /// Entries have to contain backlinks when sequence number is given.
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
        // If it is the first entry set sequence number to 1, otherwise increment
        let seq_num = match previous_seq_num {
            None => SeqNum::default(),
            Some(s) => {
                let mut next_seq_num = s.clone();
                next_seq_num.next().unwrap()
            }
        };

        let entry = Self {
            log_id: log_id.clone().to_owned(),
            message: Some(message.clone().to_owned()),
            entry_hash_skiplink: entry_hash_skiplink.cloned(),
            entry_hash_backlink: entry_hash_backlink.cloned(),
            seq_num,
        };

        entry.validate()?;

        Ok(entry)
    }

    pub fn sign_and_encode(&self, key_pair: &KeyPair) -> Result<EntryEncoded> {
        // Generate message hash
        // @TODO: Handle case where message is not set
        let message_encoded = self.message.clone().unwrap().encode().unwrap();
        let message_hash = message_encoded.hash();
        let message_size = message_encoded.size();

        // Convert entry links to Bamboo structs
        let backlink = self
            .entry_hash_backlink
            .clone()
            .map(|link| YamfHash::Blake2b(link.to_bytes()));

        let lipmaa_link = self
            .entry_hash_skiplink
            .clone()
            .map(|link| YamfHash::Blake2b(link.to_bytes()));

        // Create bamboo entry
        let mut entry: BambooEntry<_, &[u8]> = BambooEntry {
            log_id: self.log_id.as_integer(),
            is_end_of_feed: false,
            payload_hash: YamfHash::Blake2b(message_hash.to_bytes()),
            payload_size: message_size,
            author: key_pair.public,
            seq_num: self.seq_num.as_integer(),
            backlink,
            lipmaa_link,
            sig: None,
        };

        // @TODO: Sign BambooEntry and encode it
        EntryEncoded::new(String::from("dummy"))
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

    pub fn seq_num(&self) -> SeqNum {
        self.seq_num.clone()
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
            && self.seq_num.is_first())
            || (self.entry_hash_backlink.is_some()
                && self.entry_hash_skiplink.is_some()
                && !self.seq_num.is_first())
        {
            bail!(EntryError::InvalidLinks);
        }

        Ok(())
    }
}
