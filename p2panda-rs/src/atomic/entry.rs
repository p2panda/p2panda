use std::convert::{TryFrom, TryInto};

use anyhow::bail;
use arrayvec::ArrayVec;
use bamboo_rs_core::{Entry as BambooEntry, YamfHash};
use thiserror::Error;

use crate::atomic::{EntrySigned, Hash, LogId, Message, MessageEncoded, SeqNum, Validation};
use crate::Result;

/// Entry of an append-only log based on Bamboo specification. It describes the actual data in the
/// p2p network and is shared between nodes.
///
/// Bamboo entries are the main data type of p2panda. Entries are organized in a distributed,
/// single-writer append-only log structure, created and signed by holders of private keys and
/// stored inside the node database.
#[derive(Debug)]
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

/// Error types for methods of `Entry` struct.
#[allow(missing_copy_implementations)]
#[derive(Error, Debug)]
pub enum EntryError {
    /// Links should not be set when first entry in log.
    #[error("backlink and skiplink not valid for this sequence number")]
    InvalidLinks,

    /// Message needs to match payload hash of encoded entry
    #[error("message needs to match payload hash of encoded entry")]
    MessageHashMismatch,
}

impl Entry {
    /// Validates and returns a new instance of `Entry`.
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
}

impl TryFrom<(&EntrySigned, Option<&MessageEncoded>)> for Entry {
    type Error = anyhow::Error;

    fn try_from(
        (signed_entry, message_encoded): (&EntrySigned, Option<&MessageEncoded>),
    ) -> std::result::Result<Self, Self::Error> {
        let entry: BambooEntry<ArrayVec<[u8; 64]>, ArrayVec<[u8; 64]>> =
            signed_entry.try_into().unwrap();

        // Messages may be omitted because the entry still contains the message hash. If the
        // message is explicitly included we require its hash to match.
        let message = match message_encoded {
            Some(msg) => {
                let yamf_hash: YamfHash<super::hash::Blake2BArrayVec> =
                    (&msg.hash()).to_owned().try_into()?;

                if yamf_hash != entry.payload_hash {
                    bail!(EntryError::MessageHashMismatch);
                }

                Some(Message::try_from(msg)?)
            }
            None => None,
        };

        let entry_hash_backlink: Option<Hash> = match entry.backlink {
            Some(link) => Some(link.try_into().unwrap()),
            None => None,
        };

        let entry_hash_skiplink: Option<Hash> = match entry.lipmaa_link {
            Some(link) => Some(link.try_into().unwrap()),
            None => None,
        };

        Ok(Entry {
            entry_hash_backlink,
            entry_hash_skiplink,
            log_id: LogId::new(entry.log_id),
            message,
            seq_num: SeqNum::new(entry.seq_num).unwrap(),
        })
    }
}

impl Validation for Entry {
    fn validate(&self) -> Result<()> {
        // First entries do not contain any sequence number or links. Every other entry has to
        // contain all information.
        let is_valid_first_entry = self.entry_hash_backlink.is_none()
            && self.entry_hash_skiplink.is_none()
            && self.seq_num.is_first();

        let is_valid_other_entry = self.entry_hash_backlink.is_some()
            && self.entry_hash_skiplink.is_some()
            && !self.seq_num.is_first();

        if !is_valid_first_entry && !is_valid_other_entry {
            bail!(EntryError::InvalidLinks);
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
        let skiplink = Hash::new_from_bytes(vec![4, 5, 6]).unwrap();
        let backlink = Hash::new_from_bytes(vec![7, 8, 9]).unwrap();

        // The first entry in a log doesn't need and cannot have references to previous entries
        assert!(Entry::new(&LogId::default(), &message, None, None, None).is_ok());

        assert!(Entry::new(
            &LogId::default(),
            &message,
            Some(&skiplink),
            Some(&backlink),
            None
        )
        .is_err());

        // Any following entry requires backreferences
        assert!(Entry::new(
            &LogId::default(),
            &message,
            Some(&skiplink),
            Some(&backlink),
            Some(&SeqNum::new(1).unwrap())
        )
        .is_ok());

        assert!(Entry::new(
            &LogId::default(),
            &message,
            None,
            None,
            Some(&SeqNum::new(1).unwrap())
        )
        .is_err());
    }
}
