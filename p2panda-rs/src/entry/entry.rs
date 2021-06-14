use bamboo_rs_core::entry::is_lipmaa_required;
use serde::{Deserialize, Serialize};

use crate::entry::{EntryError, LogId, SeqNum};
use crate::hash::Hash;
use crate::message::Message;
use crate::Validate;

/// Entry of an append-only log based on [`Bamboo`] specification. It describes the actual data in
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
/// [`Bamboo`]: https://github.com/AljoschaMeyer/bamboo
///
/// ## Example
///
/// ```
/// # extern crate p2panda_rs;
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// use p2panda_rs::entry::{Entry, LogId, SeqNum};
/// use p2panda_rs::message::{Message, MessageFields, MessageValue};
/// use p2panda_rs::hash::Hash;
/// # let schema_hash_str = "004069db5208a271c53de8a1b6220e6a4d7fcccd89e6c0c7e75c833e34dc68d932624f2ccf27513f42fb7d0e4390a99b225bad41ba14a6297537246dbe4e6ce150e8";
///
/// // == FIRST ENTRY IN NEW LOG ==
///
/// // Create schema hash
/// let schema_hash = Hash::new(schema_hash_str)?;
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
///     Some(&message),
///     None,
///     None,
///     &SeqNum::new(1)?,
/// )?;
/// # Ok(())
/// # }
/// ```
/// ## Example
/// ```
/// # extern crate p2panda_rs;
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// use p2panda_rs::entry::{Entry, LogId, SeqNum};
/// use p2panda_rs::message::{Message, MessageFields, MessageValue};
/// use p2panda_rs::hash::Hash;
///
/// // == ENTRY IN EXISTING LOG ==
/// # let backlink_hash_string = "004069db5208a271c53de8a1b6220e6a4d7fcccd89e6c0c7e75c833e34dc68d932624f2ccf27513f42fb7d0e4390a99b225bad41ba14a6297537246dbe4e6ce150e8";
/// # let skiplink_hash_string = "004069db5208a271c53de8a1b6220e6a4d7fcccd89e6c0c7e75c833e34dc68d932624f2ccf27513f42fb7d0e4390a99b225bad41ba14a6297537246dbe4e6ce150e8";
/// # let schema_hash_string = "004069db5208a271c53de8a1b6220e6a4d7fcccd89e6c0c7e75c833e34dc68d932624f2ccf27513f42fb7d0e4390a99b225bad41ba14a6297537246dbe4e6ce150e8";
///
/// // Create schema hash
/// let schema_hash = Hash::new(schema_hash_string)?;
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
/// let skiplink_hash = Hash::new(&skiplink_hash_string)?;
///
/// // Create backlink hash from string
/// let backlink_hash = Hash::new(&backlink_hash_string)?;
///
/// // Create entry
/// let next_entry = Entry::new(
///     &log_id,
///     Some(&message),
///     Some(&skiplink_hash),
///     Some(&backlink_hash),
///     &seq_no,
/// )?;
/// # Ok(())
/// # }
/// ```
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

impl Validate for Entry {
    type Error = EntryError;

    fn validate(&self) -> Result<(), Self::Error> {
        // First entries do not contain any sequence number or links. 
        // Every other entry has to contain a backlink and skiplink unless
        // they are equal, in which case the skiplink can be omitted.

        match (
            self.seq_num.is_first(),
            self.entry_hash_backlink.is_some(),
            self.entry_hash_skiplink.is_some(),
            self.is_skiplink_required(),
        ) {
            (true, false, false, false) => Ok(()),
            (false, true, false, false) => Ok(()),
            (false, true, true, _) => Ok(()),
            (_, _, _, _) => Err(EntryError::InvalidLinks),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::entry::{LogId, SeqNum};
    use crate::hash::Hash;
    use crate::message::{Message, MessageFields, MessageValue};

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
