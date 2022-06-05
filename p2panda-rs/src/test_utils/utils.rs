// SPDX-License-Identifier: AGPL-3.0-or-later

//! Helper methods for generating common p2panda data objects.
//!
//! Used when generating fixtures and in the mock node and client implementations.
//!
//! The primary reason we separate this from the main fixture logic is that these methods can be
//! imported and used outside of testing modules, whereas the fixture macros can only be injected
//! into `rstest` defined methods.
use serde::Serialize;

use crate::entry::{LogId, SeqNum};
use crate::hash::Hash;
use crate::identity::KeyPair;
use crate::schema::SchemaId;

/// A custom `Result` type to be able to dynamically propagate `Error` types.
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// Struct which contains the values for the next entry args needed when publishing a new entry.
#[derive(Serialize, Debug)]
pub struct NextEntryArgs {
    /// The backlink of the next entry, can be None if this is the first entry published.
    pub backlink: Option<Hash>,

    /// The skiplink of the next entry, can be None if it's the same as the backlink.
    pub skiplink: Option<Hash>,

    /// The seq number for the next entry.
    pub seq_num: SeqNum,

    /// The log id of this log.
    pub log_id: LogId,
}

/// Generate a new key pair, not based on the default private key.
pub fn new_key_pair() -> KeyPair {
    KeyPair::new()
}

/// Generate a key pair from a private key.
pub fn keypair_from_private(private_key: String) -> KeyPair {
    KeyPair::from_private_key_str(&private_key).unwrap()
}

/// Generate an application schema based on a schema id string.
pub fn schema(schema_id: &str) -> SchemaId {
    SchemaId::new(schema_id).unwrap()
}
