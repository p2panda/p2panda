// SPDX-License-Identifier: AGPL-3.0-or-later

use std::convert::{TryFrom, TryInto};
use std::hash::Hash as StdHash;

use bamboo_rs_core_ed25519_yasmf::entry::is_lipmaa_required;
use bamboo_rs_core_ed25519_yasmf::Entry as BambooEntry;

use crate::entry::encode::sign_entry;
use crate::entry::error::{DecodeEntryError, EntryBuilderError};
use crate::entry::{LogId, SeqNum, Signature};
use crate::hash::Hash;
use crate::identity::{Author, KeyPair};
use crate::operation::EncodedOperation;

/// Create and sign new `Entry` instances.
#[derive(Clone, Debug, Default)]
pub struct EntryBuilder {
    /// Used log for this entry.
    log_id: LogId,

    /// Sequence number of this entry.
    seq_num: SeqNum,

    /// Hash of skiplink Bamboo entry.
    skiplink: Option<Hash>,

    /// Hash of previous Bamboo entry.
    backlink: Option<Hash>,
}

impl EntryBuilder {
    /// Returns a new instance of `EntryBuilder`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set log id of entry.
    pub fn log_id(mut self, log_id: &LogId) -> Self {
        self.log_id = log_id.to_owned();
        self
    }

    /// Set sequence number of entry.
    pub fn seq_num(mut self, seq_num: &SeqNum) -> Self {
        self.seq_num = seq_num.to_owned();
        self
    }

    /// Set skiplink hash of entry.
    pub fn skiplink(mut self, hash: &Hash) -> Self {
        self.skiplink = Some(hash.to_owned());
        self
    }

    /// Set backlink hash of entry.
    pub fn backlink(mut self, hash: &Hash) -> Self {
        self.backlink = Some(hash.to_owned());
        self
    }

    /// Signs entry and secures payload with the author's key pair, returns a new `Entry` instance.
    ///
    /// An `EncodedOperation` is required here as the entry payload. The entry is "pointing" at the
    /// payload to secure and authenticate it. Later on, the payload can theoretically be deleted
    /// when it is not needed anymore.
    ///
    /// Using this method we can assume that the entry will be correctly signed. This applies only
    /// basic checks if the backlink and skiplink is correctly set for the given sequence number
    /// (#3). Please note though that this method not check for correct log integrity!
    pub fn sign(
        mut self,
        encoded_operation: &EncodedOperation,
        key_pair: &KeyPair,
    ) -> Result<Entry, EntryBuilderError> {
        let entry = sign_entry(
            &self.log_id,
            &self.seq_num,
            self.skiplink.as_ref(),
            self.backlink.as_ref(),
            &encoded_operation,
            &key_pair,
        )?;

        Ok(entry)
    }
}

/// Entry of an append-only log based on [`Bamboo`] specification.
///
/// Bamboo entries are the main data type of p2panda. They describe the actual data in the p2p
/// network and are shared between nodes. Entries are organised in a distributed, single-writer
/// append-only log structure, created and signed by holders of private keys and stored inside the
/// node's database.
///
/// Entries are separated from the actual (off-chain) data to be able to delete application data
/// without loosing the integrity of the log. Payload data is formatted as "operations" in p2panda.
/// Each entry only holds a hash of the operation payload, this is why an [`Operation`] instance is
/// required during entry signing.
///
/// [`Bamboo`]: https://github.com/AljoschaMeyer/bamboo
#[derive(Debug, Clone, Eq, PartialEq, StdHash)]
pub struct Entry {
    /// Author of this entry.
    author: Author,

    /// Used log for this entry.
    log_id: LogId,

    /// Sequence number of this entry.
    seq_num: SeqNum,

    /// Hash of skiplink Bamboo entry.
    skiplink: Option<Hash>,

    /// Hash of previous Bamboo entry.
    backlink: Option<Hash>,

    /// Byte size of payload.
    payload_size: u64,

    /// Hash of payload.
    payload_hash: Hash,

    /// Ed25519 signature of entry.
    signature: Signature,
}

impl Entry {
    /// Returns public key of entry.
    pub fn public_key(&self) -> &Author {
        &self.author
    }

    /// Returns log id of entry.
    pub fn log_id(&self) -> &LogId {
        &self.log_id
    }

    /// Returns sequence number of entry.
    pub fn seq_num(&self) -> &SeqNum {
        &self.seq_num
    }

    /// Returns hash of skiplink entry when given.
    pub fn skiplink(&self) -> Option<&Hash> {
        self.skiplink.as_ref()
    }

    /// Returns hash of backlink entry when given.
    pub fn backlink(&self) -> Option<&Hash> {
        self.backlink.as_ref()
    }

    /// Returns payload size of operation.
    pub fn payload_size(&self) -> u64 {
        self.payload_size
    }

    /// Returns payload hash of operation.
    pub fn payload_hash(&self) -> &Hash {
        &self.payload_hash
    }

    /// Returns signature of entry.
    pub fn signature(&self) -> &Signature {
        &self.signature
    }

    /// Calculates sequence number of backlink entry.
    pub fn seq_num_backlink(&self) -> Option<SeqNum> {
        self.seq_num.backlink_seq_num()
    }

    /// Calculates sequence number of skiplink entry.
    pub fn seq_num_skiplink(&self) -> Option<SeqNum> {
        self.seq_num.skiplink_seq_num()
    }

    /// Returns true if skiplink has to be given.
    pub fn is_skiplink_required(&self) -> bool {
        is_lipmaa_required(self.seq_num.as_u64())
    }
}

impl TryFrom<BambooEntry<&[u8], &[u8]>> for Entry {
    type Error = DecodeEntryError;

    fn try_from(entry: BambooEntry<&[u8], &[u8]>) -> Result<Self, Self::Error> {
        // Convert all hashes into our types
        let backlink: Option<Hash> = match entry.backlink {
            Some(link) => Some((&link).try_into()?),
            None => None,
        };

        let skiplink: Option<Hash> = match entry.lipmaa_link {
            Some(link) => Some((&link).try_into()?),
            None => None,
        };

        let payload_hash: Hash = (&entry.payload_hash).try_into()?;

        // Unwrap as we know that there is a signature coming from bamboo
        let signature = entry.sig.expect("signature expected").into();

        Ok(Entry {
            author: (&entry.author).into(),
            log_id: entry.log_id.into(),
            seq_num: entry.seq_num.try_into()?,
            skiplink,
            backlink,
            payload_hash,
            payload_size: entry.payload_size,
            signature,
        })
    }
}

/* #[cfg(test)]
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
} */
