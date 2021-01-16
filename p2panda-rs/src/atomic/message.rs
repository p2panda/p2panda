use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::HashMap;

use crate::atomic::{Hash, MessageEncoded};
use crate::error::Result;

const MESSAGE_VERSION: u64 = 1;

#[derive(Clone, Debug, PartialEq)]
pub enum MessageAction {
    Create,
    Update,
    Delete,
}

impl Serialize for MessageAction {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(match *self {
            MessageAction::Create => "create",
            MessageAction::Update => "update",
            MessageAction::Delete => "delete",
        })
    }
}

impl<'de> Deserialize<'de> for MessageAction {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(match s.as_str() {
            "create" => MessageAction::Create,
            "update" => MessageAction::Update,
            "delete" => MessageAction::Delete,
            _ => panic!("meh"),
        })
    }
}

impl Copy for MessageAction {}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MessageFields(HashMap<String, MessageValue>);

impl MessageFields {
    pub fn new() -> Self {
        Self(HashMap::new())
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn add(&mut self, name: &str, value: MessageValue) -> Result<()> {
        if self.0.contains_key(name) {
            // @TODO: Correct error handling
            panic!("Field name already exists");
        }

        self.0.insert(name.to_owned(), value);

        Ok(())
    }

    pub fn update(&mut self, name: &str, value: MessageValue) -> Result<()> {
        if !self.0.contains_key(name) {
            // @TODO: Correct error handling
            panic!("Field name does not exist");
        }

        self.0.insert(name.to_owned(), value);

        Ok(())
    }

    pub fn remove(&mut self, name: &str) -> Result<()> {
        if !self.0.contains_key(name) {
            // @TODO: Correct error handling
            panic!("Field name does not exist");
        }

        self.0.remove(name);

        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MessageValue {
    Text(String),
}

/// Messages describe data mutations in the p2panda network. Authors send messages to create,
/// update or delete instances or collections of data.
///
/// The data itself lives in the `fields` object and is formed after a message schema.
#[derive(Debug, Serialize, Deserialize)]
pub struct Message {
    /// Describes if this message creates, updates or deletes data.
    action: MessageAction,

    /// Hash of schema describing format of message fields.
    schema: Hash,

    /// Version schema of this message.
    version: u64,

    /// Optional id referring to the data instance.
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<Hash>,

    /// Optional fields map holding the message data.
    #[serde(skip_serializing_if = "Option::is_none")]
    fields: Option<MessageFields>,
}

impl Message {
    /// Returns new create message.
    pub fn create(schema: Hash, fields: MessageFields) -> Self {
        Self {
            action: MessageAction::Create,
            version: MESSAGE_VERSION,
            schema,
            id: None,
            fields: Some(fields),
        }
    }

    /// Returns new update message.
    pub fn update(schema: Hash, id: Hash, fields: MessageFields) -> Self {
        Self {
            action: MessageAction::Update,
            version: MESSAGE_VERSION,
            schema,
            id: Some(id),
            fields: Some(fields),
        }
    }

    /// Returns new delete message.
    pub fn delete(schema: Hash, id: Hash) -> Self {
        Self {
            action: MessageAction::Delete,
            version: MESSAGE_VERSION,
            schema,
            id: Some(id),
            fields: None,
        }
    }

    pub fn from_encoded(_message_encoded: MessageEncoded) -> Result<Self> {
        todo!();
    }

    pub fn encode(&self) -> Result<MessageEncoded> {
        // Serialize data to binary CBOR format
        let cbor = serde_cbor::to_vec(&self)?;

        // Encode bytes as hex string
        let encoded = hex::encode(cbor);

        Ok(MessageEncoded::new(&encoded)?)
    }
}

#[cfg(test)]
mod tests {
    use crate::atomic::Hash;

    use super::{Message, MessageFields, MessageValue};

    #[test]
    fn encode() {
        let mut fields = MessageFields::new();
        fields
            .add("test", MessageValue::Text("Hello, Message!".to_owned()))
            .unwrap();

        let message = Message::update(
            Hash::from_bytes(vec![1, 255, 0]).unwrap(),
            Hash::from_bytes(vec![62, 128]).unwrap(),
            fields,
        );

        println!("{:#?}", message);

        let encoded = message.encode().unwrap();

        println!("{:#?}", encoded);
    }
}
