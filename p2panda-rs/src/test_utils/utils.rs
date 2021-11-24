// SPDX-License-Identifier: AGPL-3.0-or-later

/// Utility methods and constants for generating common p2panda data objects. Used when generating fixtures and in the mock node and client implementations. The primary reason we seperate this from the main fixture logic is that these methods can be imported and used outside of testing modules, whereas the fixture macros can only be injected into rstest defined methods.
use serde::Serialize;

use crate::entry::{Entry, LogId, SeqNum};
use crate::hash::Hash;
use crate::identity::KeyPair;
use crate::message::{Message, MessageFields, MessageValue};

/// A custom `Result` type to be able to dynamically propagate `Error` types.
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

/// Struct which contains the values for the next entry args needed when publishing a new entry.
#[derive(Serialize, Debug)]
pub struct NextEntryArgs {
    /// The backlink of the next entry, can be None if this is the first entry published
    pub backlink: Option<Hash>,
    /// The skiplink of the next entry, can be None if it's the same as the backlink
    pub skiplink: Option<Hash>,
    /// The seq number for the next entry
    pub seq_num: SeqNum,
    /// The log id of this log
    pub log_id: LogId,
}

/// The default hash string, used when a hash is needed for testing, it's the default hash in fixtures when a custom value isn't specified.
pub const DEFAULT_HASH: &str  = "0040cf94f6d605657e90c543b0c919070cdaaf7209c5e1ea58acb8f3568fa2114268dc9ac3bafe12af277d286fce7dc59b7c0c348973c4e9dacbe79485e56ac2a702";

/// The default schema hash string, used in all message fixtures when no custom schema hash is defined.
pub const DEFAULT_SCHEMA_HASH: &str  = "00401d76566758a5b6bfc561f1c936d8fc86b5b42ea22ab1dabf40d249d27dd906401fde147e53f44c103dd02a254916be113e51de1077a946a3a0c1272b9b348437";

/// The default private key string, used for when creating authors and public keys in fixtures.
pub const DEFAULT_PRIVATE_KEY: &str =
    "eb852fefa703901e42f17cdc2aa507947f392a72101b2c1a6d30023af14f75e2";

/// The default seq number, used when an entry is created in a fixture and no custom values are provided.
pub const DEFAULT_SEQ_NUM: i64 = 1;

/// A helper method for easily generating a message of any type ("CREATE", "UPDATE", "DELETE").
///
/// If a value for `fields` is provided, this is a CREATE message
/// If values for both `fields` and `instance_id` are provided, this is an UPDATE message
/// If no value for `fields` is provided, this is a DELETE message
pub fn any_message(fields: Option<MessageFields>, instance_id: Option<Hash>) -> Message {
    match fields {
        // It's a CREATE message
        Some(fields) if instance_id.is_none() => {
            Message::new_create(Hash::new(DEFAULT_SCHEMA_HASH).unwrap(), fields).unwrap()
        }
        // It's an UPDATE message
        Some(fields) => Message::new_update(
            Hash::new(DEFAULT_SCHEMA_HASH).unwrap(),
            instance_id.unwrap(),
            fields,
        )
        .unwrap(),
        // It's a DELETE message
        None => Message::new_delete(
            Hash::new(DEFAULT_SCHEMA_HASH).unwrap(),
            instance_id.unwrap(),
        )
        .unwrap(),
    }
}

/// Helper method for generating MessageFields from a vector of key-value tuples, currently only string types are implemented.
pub fn message_fields(fields: Vec<(&str, &str)>) -> MessageFields {
    let mut message_fields = MessageFields::new();
    for (key, value) in fields.iter() {
        message_fields
            .add(key, MessageValue::Text(value.to_string()))
            .unwrap();
    }
    message_fields
}

/// Generate a new key pair, not based on the default private key
pub fn new_key_pair() -> KeyPair {
    KeyPair::new()
}

/// Generate a key pair from a private key
pub fn keypair_from_private(private_key: String) -> KeyPair {
    KeyPair::from_private_key_str(&private_key).unwrap()
}

/// Generate a sequence number based on passed i64 value
pub fn seq_num(n: i64) -> SeqNum {
    SeqNum::new(n).unwrap()
}

/// Generate a hash based on a hash string
pub fn hash(hash_str: &str) -> Hash {
    Hash::new(hash_str).unwrap()
}

/// Generate an entry based on passed values
pub fn entry(
    message: Message,
    skiplink: Option<Hash>,
    backlink: Option<Hash>,
    seq_num: SeqNum,
) -> Entry {
    Entry::new(
        &LogId::default(),
        Some(&message),
        skiplink.as_ref(),
        backlink.as_ref(),
        &seq_num,
    )
    .unwrap()
}

/// Generate a create message based on passed schema hash and message fields
pub fn create_message(schema: Hash, fields: MessageFields) -> Message {
    Message::new_create(schema, fields).unwrap()
}

/// Generate an update message based on passed schema hash, instance id and message fields
pub fn update_message(schema: Hash, instance_id: Hash, fields: MessageFields) -> Message {
    Message::new_update(schema, instance_id, fields).unwrap()
}

/// Generate a delete message based on passed schema hash and instance id
pub fn delete_message(schema: Hash, instance_id: Hash) -> Message {
    Message::new_delete(schema, instance_id).unwrap()
}
