use std::convert::TryFrom;

use crate::atomic::{Entry, Hash, LogId, Message, MessageEncoded, MessageFields, MessageValue, SeqNum,};
use crate::key_pair::KeyPair;

use crate::encoder::{encode_entry, decode_entry, sign_and_encode, validate_message};

/// Returns a Message for testing
pub fn mock_message(text: String) -> Message {
    let mut fields = MessageFields::new();
    fields
        .add("test", MessageValue::Text(text.to_owned()))
        .unwrap();
    Message::new_create(Hash::new_from_bytes(vec![1, 2, 3]).unwrap(), fields).unwrap()
}

/// Returns an Entry for testing
pub fn mock_entry(message: Message) -> Entry {
    Entry::new(
        &LogId::default(), 
        Some(&message), 
        None, 
        None, 
        &SeqNum::new(1).unwrap())
        .unwrap()
}