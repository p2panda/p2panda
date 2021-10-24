//! Structs, hard coded values and convenience methods for creating common p2panda data. Mostly utilized in the fixtures
//! also contained in this testing module. hould not be used outside of a testing environment as best practice for
//! error checking and unwrapping is not followed.
use crate::entry::{Entry, EntrySigned, LogId, SeqNum};
use crate::hash::Hash;
use crate::identity::KeyPair;
use crate::message::{Message, MessageFields, MessageEncoded, MessageValue};

pub struct Fixture {
    pub entry: Entry,
    pub entry_signed_encoded: EntrySigned,
    pub key_pair: KeyPair,
    pub message_encoded: MessageEncoded,
}

pub const MESSAGE_SCHEMA: &str  = "00401d76566758a5b6bfc561f1c936d8fc86b5b42ea22ab1dabf40d249d27dd906401fde147e53f44c103dd02a254916be113e51de1077a946a3a0c1272b9b348437";

pub const DEFAULT_HASH: &str  = "0040cf94f6d605657e90c543b0c919070cdaaf7209c5e1ea58acb8f3568fa2114268dc9ac3bafe12af277d286fce7dc59b7c0c348973c4e9dacbe79485e56ac2a702";

pub const DEFAULT_PRIVATE_KEY: &str = "eb852fefa703901e42f17cdc2aa507947f392a72101b2c1a6d30023af14f75e2";

pub const DEFAULT_SEQ_NUM: i64 = 1;

pub fn new_key_pair() -> KeyPair {
    KeyPair::new()
}

pub fn keypair_from_private(private_key: String) -> KeyPair {
    KeyPair::from_private_key_str(&private_key).unwrap()
}

pub fn seq_num(n: i64) -> SeqNum {
    SeqNum::new(n).unwrap()
}

pub fn schema(schema_str: &str) -> String {
    schema_str.to_owned()
}

pub fn hash(hash_str: &str) -> Hash {
    Hash::new(hash_str).unwrap()
}

pub fn fields(fields_vec: Vec<(&str, &str)>) -> MessageFields {
    let mut message_fields = MessageFields::new();
    for (key, value) in fields_vec.iter() {
        message_fields
            .add(key, MessageValue::Text(value.to_string()))
            .unwrap();
    }
    message_fields
}

pub fn any_message(
    fields: Option<MessageFields>,
    instance_id: Option<Hash>,
) -> Message {
    match fields {
        // It's a CREATE message
        Some(fields) if instance_id.is_none() => {    
            Message::new_create(Hash::new(MESSAGE_SCHEMA).unwrap(), fields).unwrap()
        },
        // It's an UPDATE message
        Some(fields) => {
            Message::new_update(
            Hash::new(MESSAGE_SCHEMA).unwrap(),
            instance_id.unwrap(),
            fields,
        )
        .unwrap()},
        // It's a DELETE message
        None if instance_id.is_some() => {
            Message::new_delete(Hash::new(MESSAGE_SCHEMA).unwrap(), instance_id.unwrap()).unwrap()
        }
        // It's a mistake....
        None => todo!(), // Error....
    }
}

pub fn build_message_fields(fields: Vec<(&str, &str)>) -> MessageFields {
    let mut message_fields = MessageFields::new();
    for (key, value) in fields.iter() {
        message_fields
            .add(key, MessageValue::Text(value.to_string()))
            .unwrap();
    }
    message_fields
}

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

pub fn create_message(schema: String, fields: MessageFields) -> Message {
    Message::new_create(Hash::new(&schema).unwrap(), fields).unwrap()
}

pub fn update_message(schema: String, instance_id: Hash, fields: MessageFields) -> Message {
    Message::new_update(Hash::new(&schema).unwrap(), instance_id, fields).unwrap()
}

pub fn delete_message(schema: String, instance_id: Hash) -> Message {
    Message::new_delete(Hash::new(&schema).unwrap(), instance_id).unwrap()
}
