use bamboo_rs_core::entry::is_lipmaa_required;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::atomic::{Hash, LogId, Message, SeqNum, Validation};

/// Error types for methods of `Entry` struct.
#[allow(missing_copy_implementations)]
#[derive(Error, Debug)]
pub enum EntryError {
    /// Links should not be set when first entry in log.
    #[error("backlink and skiplink not valid for this sequence number")]
    InvalidLinks,

    /// Handle errors from [`atomic::Hash`] struct.
    #[error(transparent)]
    HashError(#[from] crate::atomic::error::HashError),

    /// Handle errors from [`atomic::SeqNum`] struct.
    #[error(transparent)]
    SeqNumError(#[from] crate::atomic::error::SeqNumError),
}

/// Entry of an append-only log based on [`Bamboo specification`]. It describes the actual data in
/// the p2p network and is shared between nodes.
///
/// Bamboo entries are the main data type of p2panda. Entries are organized in a distributed,
/// single-writer append-only log structure, created and signed by holders of private keys and
/// stored inside the node database.
///
/// Entries are separated from the actual (off-chain) data to be able to delete user data without
/// loosing the integrity of the log. Each entry only holds a hash of the message payload, this is
/// why a message instance is required during entry signing.
///
/// [`Bamboo specification`]: https://github.com/AljoschaMeyer/bamboo
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    /// Hash of previous Bamboo entry.
    entry_hash_backlink: Option<Hash>,

    /// Hash of skiplink Bamboo entry.
    entry_hash_skiplink: Option<Hash>,

    /// Used log for this entry.
    log_id: LogId,

    /// Message payload of entry, can be deleted.
    message: Option<Message>,

    /// Sequence number of this entry.
    seq_num: SeqNum,
}

impl Entry {
    /// Validates and returns a new instance of `Entry`.
    pub fn new(
        log_id: &LogId,
        message: Option<&Message>,
        entry_hash_skiplink: Option<&Hash>,
        entry_hash_backlink: Option<&Hash>,
        seq_num: &SeqNum,
    ) -> Result<Self, EntryError> {
        let entry = Self {
            log_id: log_id.clone().to_owned(),
            message: message.cloned(),
            entry_hash_skiplink: entry_hash_skiplink.cloned(),
            entry_hash_backlink: entry_hash_backlink.cloned(),
            seq_num: seq_num.clone(),
        };
        entry.validate()?;

        Ok(entry)
    }

    /// Returns hash of backlink entry when given.
    pub fn backlink_hash(&self) -> Option<&Hash> {
        self.entry_hash_backlink.as_ref()
    }

    /// Returns hash of skiplink entry when given.
    pub fn skiplink_hash(&self) -> Option<&Hash> {
        self.entry_hash_skiplink.as_ref()
    }

    /// Returns sequence number of entry.
    pub fn seq_num(&self) -> &SeqNum {
        &self.seq_num
    }

    /// Calculates sequence number of backlink entry.
    pub fn seq_num_backlink(&self) -> Option<SeqNum> {
        self.seq_num.backlink_seq_num()
    }

    /// Calculates sequence number of skiplink entry.
    pub fn seq_num_skiplink(&self) -> Option<SeqNum> {
        self.seq_num.skiplink_seq_num()
    }

    /// Returns message of entry.
    pub fn message(&self) -> Option<&Message> {
        self.message.as_ref()
    }

    /// Returns log_id of entry.
    pub fn log_id(&self) -> &LogId {
        &self.log_id
    }

    /// Returns true if entry contains message.
    pub fn has_message(&self) -> bool {
        self.message.is_some()
    }

    /// Returns true if skiplink has to be given.
    pub fn is_skiplink_required(&self) -> bool {
        is_lipmaa_required(self.seq_num.as_i64() as u64)
    }
}

impl Validation for Entry {
    type Error = EntryError;

    fn validate(&self) -> Result<(), Self::Error> {
        // First entries do not contain any sequence number or links. Every other entry has to
        // contain all information.
        let is_valid_first_entry = self.entry_hash_backlink.is_none()
            && self.entry_hash_skiplink.is_none()
            && self.seq_num.is_first();

        let is_valid_other_entry = if !self.seq_num.is_first() && self.entry_hash_backlink.is_some()
        {
            // Skiplink can be empty when same as backlink
            (self.is_skiplink_required() && self.entry_hash_skiplink.is_some())
                || !self.is_skiplink_required()
        } else {
            false
        };

        if !is_valid_first_entry && !is_valid_other_entry {
            return Err(EntryError::InvalidLinks);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {

    use crate::atomic::{Hash, LogId, Message, MessageFields, MessageValue, SeqNum};

    use super::Entry;

    #[test]
    fn validation() {
        // Prepare sample values
        let mut fields = MessageFields::new();
        fields
            .add("test", MessageValue::Text("Hello".to_owned()))
            .unwrap();
        let message =
            Message::new_create(Hash::new_from_bytes(vec![1, 2, 3]).unwrap(), fields).unwrap();
        let backlink = Hash::new_from_bytes(vec![7, 8, 9]).unwrap();

        // The first entry in a log doesn't need and cannot have references to previous entries
        assert!(Entry::new(
            &LogId::default(),
            Some(&message),
            None,
            None,
            &SeqNum::new(1).unwrap()
        )
        .is_ok());

        // Try to pass them over anyways, it will be invalidated
        assert!(Entry::new(
            &LogId::default(),
            Some(&message),
            Some(&backlink.clone()),
            Some(&backlink),
            &SeqNum::new(1).unwrap()
        )
        .is_err());

        // Any following entry requires backreferences
        assert!(Entry::new(
            &LogId::default(),
            Some(&message),
            Some(&backlink.clone()),
            Some(&backlink),
            &SeqNum::new(2).unwrap()
        )
        .is_ok());

        // We can omit the skiplink here as it is the same as the backlink
        assert!(Entry::new(
            &LogId::default(),
            Some(&message),
            None,
            Some(&backlink),
            &SeqNum::new(2).unwrap()
        )
        .is_ok());

        // We need a backlink here
        assert!(Entry::new(
            &LogId::default(),
            Some(&message),
            None,
            None,
            &SeqNum::new(2).unwrap()
        )
        .is_err());
    }
}
