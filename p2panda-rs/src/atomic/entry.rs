use std::convert::{TryFrom, TryInto};

use arrayvec::ArrayVec;
use bamboo_rs_core::entry::is_lipmaa_required;
use bamboo_rs_core::{Entry as BambooEntry, YamfHash};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::atomic::{EntrySigned, Hash, LogId, Message, MessageEncoded, SeqNum, Validation};

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
///
/// ## Example
///
/// ```
/// # extern crate p2panda_rs;
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// use p2panda_rs::atomic::{Entry, Hash, LogId, Message, MessageFields, MessageValue, SeqNum};
/// # let SCHEMA_HASH_STR = "004069db5208a271c53de8a1b6220e6a4d7fcccd89e6c0c7e75c833e34dc68d932624f2ccf27513f42fb7d0e4390a99b225bad41ba14a6297537246dbe4e6ce150e8";
///
/// // == FIRST ENTRY IN NEW LOG == //
///
/// // Create schema hash
/// let schema_hash = Hash::new(SCHEMA_HASH_STR)?;
///
/// // Create a MessageFields instance and add a text field string with the key "title"
/// let mut fields = MessageFields::new();
/// fields.add("title", MessageValue::Text("Hello, Panda!".to_owned()))?;
///
/// // Create a message containing the above fields
/// let message = Message::new_create(schema_hash, fields)?;
///
/// // Create the first Entry in a log
/// let entry = Entry::new(
///     &LogId::default(),
///     &message,
///     None,
///     None,
///     None,
/// )?;
/// # Ok(())
/// # }
///```
/// ## Example
///```
/// # extern crate p2panda_rs;
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// use p2panda_rs::atomic::{Entry, Hash, LogId, Message, MessageFields, MessageValue, SeqNum};
///
/// // == ENTRY IN EXISTING LOG ==
/// # let BACKLINK_HASH_STR = "004069db5208a271c53de8a1b6220e6a4d7fcccd89e6c0c7e75c833e34dc68d932624f2ccf27513f42fb7d0e4390a99b225bad41ba14a6297537246dbe4e6ce150e8";
/// # let SKIPLINK_HASH_STR = "004069db5208a271c53de8a1b6220e6a4d7fcccd89e6c0c7e75c833e34dc68d932624f2ccf27513f42fb7d0e4390a99b225bad41ba14a6297537246dbe4e6ce150e8";
/// # let SCHEMA_HASH_STR = "004069db5208a271c53de8a1b6220e6a4d7fcccd89e6c0c7e75c833e34dc68d932624f2ccf27513f42fb7d0e4390a99b225bad41ba14a6297537246dbe4e6ce150e8";
/// 
/// // Create schema hash
/// let schema_hash = Hash::new(SCHEMA_HASH_STR)?;
///
/// // Create a MessageFields instance and add a text field string with the key "title"
/// let mut fields = MessageFields::new();
/// fields.add("title", MessageValue::Text("Hello, Panda!".to_owned()))?;
///
/// // Create a message containing the above fields
/// let message = Message::new_create(schema_hash, fields)?;
///
/// // Create log ID from i64
/// let log_id = LogId::new(1);
///
/// // Create sequence number from i64
/// let seq_no = SeqNum::new(2)?;
///
/// // Create skiplink hash from string
/// let skiplink_hash = Hash::new(&SKIPLINK_HASH_STR)?;
///
/// // Create backlink hash from string
/// let backlink_hash = Hash::new(&BACKLINK_HASH_STR)?;
///
/// // Create entry
/// let next_entry = Entry::new(
///     &log_id,
///     &message,
///     Some(&skiplink_hash),
///     Some(&backlink_hash),
///     Some(&seq_no),
/// )?;
/// # Ok(())
/// # }
/// ```
///


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
        message: &Message,
        entry_hash_skiplink: Option<&Hash>,
        entry_hash_backlink: Option<&Hash>,
        previous_seq_num: Option<&SeqNum>,
    ) -> Result<Self, EntryError> {
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

    /// Returns true if skiplink has to be given.
    pub fn is_skiplink_required(&self) -> bool {
        is_lipmaa_required(self.seq_num.as_i64() as u64)
    }
}

/// Takes an encoded and signed [`EntrySigned`] and converts it back to its original, unsigned and
/// decoded `Entry` state.
///
/// This conversion is lossy as the Signature will be removed.
///
/// Entries are separated from the messages they refer to. Since messages can independently be
/// deleted they can be passed on optionally during the conversion. When a [`Message`] exists this
/// conversion will automatically check its integrity with this Entry by comparing their hashes.
impl TryFrom<(&EntrySigned, Option<&MessageEncoded>)> for Entry {
    type Error = EntryError;

    fn try_from(
        (signed_entry, message_encoded): (&EntrySigned, Option<&MessageEncoded>),
    ) -> Result<Self, Self::Error> {
        // Convert to Entry from bamboo_rs_core first
        let entry: BambooEntry<ArrayVec<[u8; 64]>, ArrayVec<[u8; 64]>> = signed_entry.into();

        // Messages may be omitted because the entry still contains the message hash. If the
        // message is explicitly included we require its hash to match.
        let message = match message_encoded {
            Some(msg) => {
                let yamf_hash: YamfHash<super::hash::Blake2BArrayVec> =
                    (&msg.hash()).to_owned().into();

                if yamf_hash != entry.payload_hash {
                    return Err(EntryError::MessageHashMismatch);
                }

                Some(Message::from(msg))
            }
            None => None,
        };

        let entry_hash_backlink: Option<Hash> = match entry.backlink {
            Some(link) => Some(link.try_into()?),
            None => None,
        };

        let entry_hash_skiplink: Option<Hash> = match entry.lipmaa_link {
            Some(link) => Some(link.try_into()?),
            None => None,
        };

        Ok(Entry {
            entry_hash_backlink,
            entry_hash_skiplink,
            log_id: LogId::new(entry.log_id as i64),
            message,
            seq_num: SeqNum::new(entry.seq_num as i64)?,
        })
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
        assert!(Entry::new(&LogId::default(), &message, None, None, None).is_ok());

        // Try to pass them over anyways, it will be invalidated
        assert!(Entry::new(
            &LogId::default(),
            &message,
            Some(&backlink.clone()),
            Some(&backlink),
            None
        )
        .is_err());

        // Any following entry requires backreferences
        assert!(Entry::new(
            &LogId::default(),
            &message,
            Some(&backlink.clone()),
            Some(&backlink),
            Some(&SeqNum::new(1).unwrap())
        )
        .is_ok());

        // We can omit the skiplink here as it is the same as the backlink
        assert!(Entry::new(
            &LogId::default(),
            &message,
            None,
            Some(&backlink),
            Some(&SeqNum::new(1).unwrap())
        )
        .is_ok());

        // We need a backlink here
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
