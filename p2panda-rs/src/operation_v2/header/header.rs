// SPDX-License-Identifier: AGPL-3.0-or-later

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::document::DocumentViewId;
use crate::hash_v2::Hash;
use crate::identity_v2::{KeyPair, PublicKey, Signature};
use crate::operation_v2::body::EncodedBody;
use crate::operation_v2::header::encode::sign_header;
use crate::operation_v2::header::error::EntryBuilderError;
use crate::operation_v2::header::traits::AsEntry;

pub type PayloadHash = Hash;

pub type PayloadSize = u64;

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct Header(
    pub(crate) HeaderVersion,
    pub(crate) PublicKey,
    pub(crate) PayloadHash,
    pub(crate) PayloadSize,
    pub(crate) HeaderExtension,
    pub(crate) Signature,
);

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum HeaderVersion {
    V1,
}

impl HeaderVersion {
    /// Returns the operation version encoded as u64.
    pub fn as_u64(&self) -> u64 {
        match self {
            HeaderVersion::V1 => 1,
        }
    }
}

impl Serialize for HeaderVersion {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u64(self.as_u64())
    }
}

impl<'de> Deserialize<'de> for HeaderVersion {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let version = u64::deserialize(deserializer)?;

        match version {
            1 => Ok(HeaderVersion::V1),
            _ => Err(serde::de::Error::custom(format!(
                "unsupported operation header version {}",
                version
            ))),
        }
    }
}

#[derive(Debug, Default, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct HeaderExtension {
    #[serde(rename = "s", skip_serializing_if = "Option::is_none")]
    seq_num: Option<u64>,

    #[serde(rename = "p", skip_serializing_if = "Option::is_none")]
    previous: Option<DocumentViewId>,

    #[serde(rename = "t", skip_serializing_if = "Option::is_none")]
    timestamp: Option<u64>,
}

#[derive(Clone, Debug, Default)]
pub struct HeaderBuilder {
    seq_num: Option<u64>,
    timestamp: Option<u64>,
    previous: Option<DocumentViewId>,
}

impl HeaderBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn seq_num(mut self, seq_num: u64) -> Self {
        self.seq_num = Some(seq_num);
        self
    }

    pub fn timestamp(mut self, timestamp: u64) -> Self {
        self.timestamp = Some(timestamp);
        self
    }

    pub fn previous(mut self, previous: &DocumentViewId) -> Self {
        self.previous = Some(previous.to_owned());
        self
    }

    pub fn sign(
        &self,
        encoded_body: &EncodedBody,
        key_pair: &KeyPair,
    ) -> Result<Header, EntryBuilderError> {
        let extension = HeaderExtension::default();

        let header = sign_header(encoded_body, &extension, key_pair)?;

        Ok(header)
    }
}

/* #[cfg(test)]
mod tests {
    use rstest::rstest;

    use crate::entry::traits::AsEntry;
    use crate::entry::{LogId, SeqNum};
    use crate::hash::Hash;
    use crate::identity::KeyPair;
    use crate::operation::EncodedOperation;
    use crate::test_utils::fixtures::{encoded_operation, key_pair, random_hash};

    use super::HeaderBuilder;

    #[rstest]
    fn entry_builder(
        #[from(random_hash)] entry_hash: Hash,
        encoded_operation: EncodedOperation,
        key_pair: KeyPair,
    ) {
        let log_id = LogId::new(92);
        let seq_num = SeqNum::new(14002).unwrap();

        let entry = HeaderBuilder::new()
            .log_id(&log_id)
            .seq_num(&seq_num)
            .backlink(&entry_hash)
            .sign(&encoded_operation, &key_pair)
            .unwrap();

        assert_eq!(entry.public_key(), &key_pair.public_key());
        assert_eq!(entry.log_id(), &log_id);
        assert_eq!(entry.seq_num(), &seq_num);
        assert_eq!(entry.skiplink(), None);
        assert_eq!(entry.backlink(), Some(&entry_hash));
        assert_eq!(entry.payload_hash(), &encoded_operation.hash());
        assert_eq!(entry.payload_size(), encoded_operation.size());
    }

    #[rstest]
    fn entry_builder_validation(
        #[from(random_hash)] entry_hash_1: Hash,
        #[from(random_hash)] entry_hash_2: Hash,
        encoded_operation: EncodedOperation,
        key_pair: KeyPair,
    ) {
        // The first entry in a log doesn't need and cannot have references to previous entries
        assert!(HeaderBuilder::new()
            .sign(&encoded_operation, &key_pair)
            .is_ok());

        // Can not have back- and skiplinks on first entry
        assert!(HeaderBuilder::new()
            .skiplink(&entry_hash_1)
            .backlink(&entry_hash_2)
            .sign(&encoded_operation, &key_pair)
            .is_err());

        // Needs backlink on second entry
        assert!(HeaderBuilder::new()
            .seq_num(&SeqNum::new(2).unwrap())
            .backlink(&entry_hash_1)
            .sign(&encoded_operation, &key_pair)
            .is_ok());

        assert!(HeaderBuilder::new()
            .seq_num(&SeqNum::new(2).unwrap())
            .sign(&encoded_operation, &key_pair)
            .is_err());

        // Needs skiplink on forth entry
        assert!(HeaderBuilder::new()
            .seq_num(&SeqNum::new(4).unwrap())
            .backlink(&entry_hash_1)
            .skiplink(&entry_hash_2)
            .sign(&encoded_operation, &key_pair)
            .is_ok());

        assert!(HeaderBuilder::new()
            .seq_num(&SeqNum::new(4).unwrap())
            .backlink(&entry_hash_1)
            .sign(&encoded_operation, &key_pair)
            .is_err());
    }

    #[rstest]
    fn entry_links_methods(
        #[from(random_hash)] entry_hash_1: Hash,
        #[from(random_hash)] entry_hash_2: Hash,
        encoded_operation: EncodedOperation,
        key_pair: KeyPair,
    ) {
        // First entry does not return any backlink or skiplink sequence number
        let entry = HeaderBuilder::new()
            .sign(&encoded_operation, &key_pair)
            .unwrap();

        assert_eq!(entry.seq_num_backlink(), None);
        // @TODO: This fails ..
        // https://github.com/p2panda/p2panda/issues/417
        // assert_eq!(entry.seq_num_skiplink(), None);
        assert!(!entry.is_skiplink_required());

        // Second entry returns sequence number for backlink
        let entry = HeaderBuilder::new()
            .seq_num(&SeqNum::new(2).unwrap())
            .backlink(&entry_hash_1)
            .sign(&encoded_operation, &key_pair)
            .unwrap();

        assert_eq!(entry.seq_num_backlink(), Some(SeqNum::new(1).unwrap()));
        // @TODO: This fails ..
        // https://github.com/p2panda/p2panda/issues/417
        // assert_eq!(entry.seq_num_skiplink(), None);
        assert!(!entry.is_skiplink_required());

        // Fourth entry returns sequence number for backlink and skiplink
        let entry = HeaderBuilder::new()
            .seq_num(&SeqNum::new(4).unwrap())
            .backlink(&entry_hash_1)
            .skiplink(&entry_hash_2)
            .sign(&encoded_operation, &key_pair)
            .unwrap();

        assert_eq!(entry.seq_num_backlink(), Some(SeqNum::new(3).unwrap()));
        assert_eq!(entry.seq_num_skiplink(), Some(SeqNum::new(1).unwrap()));
        assert!(entry.is_skiplink_required());
    }
} */
