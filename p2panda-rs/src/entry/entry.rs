// SPDX-License-Identifier: AGPL-3.0-or-later

use bamboo_rs_core_ed25519_yasmf::entry::is_lipmaa_required;

use crate::entry::{EntryError, LogId, SeqNum};
use crate::hash::Hash;
use crate::operation::Operation;
use crate::Validate;

/// Entry of an append-only log based on [`Bamboo`] specification.
///
/// Bamboo entries are the main data type of p2panda. They describe the actual data in the p2p
/// network and are shared between nodes. Entries are organised in a distributed, single-writer
/// append-only log structure, created and signed by holders of private keys and stored inside the
/// node database.
///
/// Entries are separated from the actual (off-chain) data to be able to delete application data
/// without loosing the integrity of the log. Payload data is formatted as "operations" in p2panda.
/// Each entry only holds a hash of the operation payload, this is why an [`Operation`] instance is
/// required during entry signing.
///
/// [`Bamboo`]: https://github.com/AljoschaMeyer/bamboo
///
/// ## Example
///
/// ```
/// # extern crate p2panda_rs;
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// use p2panda_rs::entry::{Entry, LogId, SeqNum};
/// use p2panda_rs::operation::{Operation, OperationFields, OperationValue};
/// use p2panda_rs::hash::Hash;
/// use p2panda_rs::schema::SchemaId;
/// # let schema_id = "chat_0020c65567ae37efea293e34a9c7d13f8f2bf23dbdc3b5c7b9ab46293111c48fc78b";
///
/// // == FIRST ENTRY IN NEW LOG ==
///
/// // Create schema id
/// let schema_id = SchemaId::new(schema_id)?;
///
/// // Create a OperationFields instance and add a text field string with the key "title"
/// let mut fields = OperationFields::new();
/// fields.add("title", OperationValue::Text("Hello, Panda!".to_owned()))?;
///
/// // Create an operation containing the above fields
/// let operation = Operation::new_create(schema_id, fields)?;
///
/// // Create the first Entry in a log
/// let entry = Entry::new(
///     &LogId::default(),
///     Some(&operation),
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
/// use p2panda_rs::operation::{Operation, OperationFields, OperationValue};
/// use p2panda_rs::hash::Hash;
/// use p2panda_rs::schema::SchemaId;
///
/// // == ENTRY IN EXISTING LOG ==
/// # let backlink_hash_string = "0020b177ec1bf26dfb3b7010d473e6d44713b29b765b99c6e60ecbfae742de496543";
/// # let schema_id = "chat_0020c65567ae37efea293e34a9c7d13f8f2bf23dbdc3b5c7b9ab46293111c48fc78b";
///
/// // Create schema
/// let schema_id = SchemaId::new(schema_id)?;
///
/// // Create a OperationFields instance and add a text field string with the key "title"
/// let mut fields = OperationFields::new();
/// fields.add("title", OperationValue::Text("Hello, Panda!".to_owned()))?;
///
/// // Create an operation containing the above fields
/// let operation = Operation::new_create(schema_id, fields)?;
///
/// // Create log ID from u64
/// let log_id = LogId::new(1);
///
/// // Create sequence number from u64
/// let seq_no = SeqNum::new(2)?;
///
/// // Create backlink hash from string
/// let backlink_hash = Hash::new(&backlink_hash_string)?;
///
/// // Create entry
/// let next_entry = Entry::new(
///     &log_id,
///     Some(&operation),
///     None,
///     Some(&backlink_hash),
///     &seq_no,
/// )?;
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct Entry {
    /// Hash of previous Bamboo entry.
    entry_hash_backlink: Option<Hash>,

    /// Hash of skiplink Bamboo entry.
    entry_hash_skiplink: Option<Hash>,

    /// Used log for this entry.
    log_id: LogId,

    /// Operation payload of entry, can be deleted.
    operation: Option<Operation>,

    /// Sequence number of this entry.
    seq_num: SeqNum,
}

impl Entry {
    /// Validates and returns a new instance of `Entry`.
    pub fn new(
        log_id: &LogId,
        operation: Option<&Operation>,
        entry_hash_skiplink: Option<&Hash>,
        entry_hash_backlink: Option<&Hash>,
        seq_num: &SeqNum,
    ) -> Result<Self, EntryError> {
        let entry = Self {
            log_id: log_id.clone().to_owned(),
            operation: operation.cloned(),
            entry_hash_skiplink: entry_hash_skiplink.cloned(),
            entry_hash_backlink: entry_hash_backlink.cloned(),
            seq_num: *seq_num,
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

    /// Returns operation of entry.
    pub fn operation(&self) -> Option<&Operation> {
        self.operation.as_ref()
    }

    /// Returns log id of entry.
    pub fn log_id(&self) -> &LogId {
        &self.log_id
    }

    /// Returns true if entry contains operation.
    pub fn has_operation(&self) -> bool {
        self.operation.is_some()
    }

    /// Returns true if skiplink has to be given.
    pub fn is_skiplink_required(&self) -> bool {
        is_lipmaa_required(self.seq_num.as_u64())
    }
}

impl Validate for Entry {
    type Error = EntryError;

    fn validate(&self) -> Result<(), Self::Error> {
        // First entries do not contain any sequence number or links. Every other entry has to
        // contain a backlink and skiplink unless they are equal, in which case the skiplink can be
        // omitted.
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
    use rstest::rstest;

    use crate::entry::{LogId, SeqNum};
    use crate::hash::Hash;
    use crate::operation::{Operation, OperationFields, OperationValue};
    use crate::schema::SchemaId;
    use crate::test_utils::fixtures::{entry, schema_id};
    use crate::Validate;

    use super::Entry;

    #[rstest]
    fn validation(schema_id: SchemaId) {
        // Prepare sample values
        let mut fields = OperationFields::new();
        fields
            .add("test", OperationValue::Text("Hello".to_owned()))
            .unwrap();
        let operation = Operation::new_create(schema_id, fields).unwrap();
        let backlink = Hash::new_from_bytes(vec![7, 8, 9]).unwrap();

        // The first entry in a log doesn't need and cannot have references to previous entries
        assert!(Entry::new(
            &LogId::default(),
            Some(&operation),
            None,
            None,
            &SeqNum::new(1).unwrap()
        )
        .is_ok());

        // Try to pass them over anyways, it will be invalidated
        assert!(Entry::new(
            &LogId::default(),
            Some(&operation),
            Some(&backlink),
            Some(&backlink),
            &SeqNum::new(1).unwrap()
        )
        .is_err());

        // Any following entry requires backlinks
        assert!(Entry::new(
            &LogId::default(),
            Some(&operation),
            Some(&backlink),
            Some(&backlink),
            &SeqNum::new(2).unwrap()
        )
        .is_ok());

        // We can omit the skiplink here as it is the same as the backlink
        assert!(Entry::new(
            &LogId::default(),
            Some(&operation),
            None,
            Some(&backlink),
            &SeqNum::new(2).unwrap()
        )
        .is_ok());

        // We need a backlink here
        assert!(Entry::new(
            &LogId::default(),
            Some(&operation),
            None,
            None,
            &SeqNum::new(2).unwrap()
        )
        .is_err());
    }

    #[rstest]
    pub fn validate_many(entry: Entry) {
        assert!(entry.validate().is_ok())
    }
}
