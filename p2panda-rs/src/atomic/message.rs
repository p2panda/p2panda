use anyhow::bail;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_repr::{Deserialize_repr, Serialize_repr};
use std::collections::HashMap;
use thiserror::Error;

use crate::atomic::{Hash, MessageEncoded};
use crate::error::Result;

/// Message format versions to introduce API changes in the future.
#[derive(Clone, Debug, PartialEq, Serialize_repr, Deserialize_repr)]
#[serde(untagged)]
#[repr(u8)]
pub enum MessageVersion {
    /// All messages are currently implemented against this first version.
    Default = 1,
}

impl Copy for MessageVersion {}

/// Messages are categorized by their `action` type.
///
/// An action defines the message format and if a data instance gets created, updated or deleted.
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

        match s.as_str() {
            "create" => Ok(MessageAction::Create),
            "update" => Ok(MessageAction::Update),
            "delete" => Ok(MessageAction::Delete),
            _ => Err(serde::de::Error::custom("unknown message action")),
        }
    }
}

impl Copy for MessageAction {}

/// Enum of possible data types which can be added to the messages fields as values.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MessageValue {
    Boolean(bool),
    Float(f64),
    Integer(i64),
    Text(String),
}

/// The actual user data is contained in form of message fields, a simple key/value store of data
/// with a limited amount of value types.
///
/// MessageFields are created separately and get attached to a Message before it is used.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MessageFields(HashMap<String, MessageValue>);

#[derive(Error, Debug)]
pub enum MessageFieldsError {
    #[error("field already exists")]
    FieldDuplicate,

    #[error("field does not exist")]
    UnknownField,
}

impl MessageFields {
    /// Creates a new fields instance to add data to.
    pub fn new() -> Self {
        Self(HashMap::new())
    }

    /// Returns the number of added fields.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns true when no field is given.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Adds a new field to this instance.
    ///
    /// A field is a simple key/value pair.
    pub fn add(&mut self, name: &str, value: MessageValue) -> Result<()> {
        if self.0.contains_key(name) {
            bail!(MessageFieldsError::FieldDuplicate);
        }

        self.0.insert(name.to_owned(), value);

        Ok(())
    }

    /// Overwrites an already existing field with a new value.
    pub fn update(&mut self, name: &str, value: MessageValue) -> Result<()> {
        if !self.0.contains_key(name) {
            bail!(MessageFieldsError::UnknownField);
        }

        self.0.insert(name.to_owned(), value);

        Ok(())
    }

    /// Removes an existing field from this instance.
    pub fn remove(&mut self, name: &str) -> Result<()> {
        if !self.0.contains_key(name) {
            bail!(MessageFieldsError::UnknownField);
        }

        self.0.remove(name);

        Ok(())
    }

    /// Returns a field value.
    pub fn get(&self, name: &str) -> Option<&MessageValue> {
        if !self.0.contains_key(name) {
            return None;
        }

        self.0.get(name)
    }
}
/// Messages describe data mutations in the p2panda network. Authors send messages to create,
/// update or delete instances or collections of data.
///
/// The data itself lives in the `fields` object and is formed after a message schema.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Message {
    /// Describes if this message creates, updates or deletes data.
    action: MessageAction,

    /// Hash of schema describing format of message fields.
    schema: Hash,

    /// Version schema of this message.
    version: MessageVersion,

    /// Optional id referring to the data instance.
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<Hash>,

    /// Optional fields map holding the message data.
    #[serde(skip_serializing_if = "Option::is_none")]
    fields: Option<MessageFields>,
}

#[derive(Error, Debug)]
pub enum MessageError {
    #[error("message fields can not be empty")]
    EmptyFields,
}

impl Message {
    /// Returns new create message.
    pub fn create(schema: Hash, fields: MessageFields) -> Result<Self> {
        if fields.is_empty() {
            bail!(MessageError::EmptyFields);
        }

        Ok(Self {
            action: MessageAction::Create,
            version: MessageVersion::Default,
            schema,
            id: None,
            fields: Some(fields),
        })
    }

    /// Returns new update message.
    pub fn update(schema: Hash, id: Hash, fields: MessageFields) -> Result<Self> {
        if fields.is_empty() {
            bail!(MessageError::EmptyFields);
        }

        Ok(Self {
            action: MessageAction::Update,
            version: MessageVersion::Default,
            schema,
            id: Some(id),
            fields: Some(fields),
        })
    }

    /// Returns new delete message.
    pub fn delete(schema: Hash, id: Hash) -> Result<Self> {
        Ok(Self {
            action: MessageAction::Delete,
            version: MessageVersion::Default,
            schema,
            id: Some(id),
            fields: None,
        })
    }

    /// Decodes an encoded message and returns it.
    pub fn from_encoded(message_encoded: MessageEncoded) -> Self {
        message_encoded.decode()
    }

    /// Encodes message in CBOR format and returns bytes.
    pub fn as_cbor(&self) -> Vec<u8> {
        // Serialize data to binary CBOR format
        serde_cbor::to_vec(&self).unwrap()
    }

    /// Returns action type of message.
    pub fn action(&self) -> MessageAction {
        self.action
    }

    /// Returns true when instance is create message.
    pub fn is_create(&self) -> bool {
        self.action == MessageAction::Create
    }

    /// Returns true when instance is update message.
    pub fn is_update(&self) -> bool {
        self.action == MessageAction::Update
    }

    /// Returns true when instance is delete message.
    pub fn is_delete(&self) -> bool {
        self.action == MessageAction::Delete
    }

    /// Returns version of message.
    pub fn version(&self) -> MessageVersion {
        self.version
    }

    /// Returns schema of message.
    pub fn schema(&self) -> Hash {
        self.schema.clone()
    }

    /// Returns id of message.
    pub fn id(&self) -> Option<Hash> {
        if self.id.is_none() {
            return None;
        }

        self.id.clone()
    }

    /// Returns true when message contains an id.
    pub fn has_id(&self) -> bool {
        self.id.is_some()
    }

    /// Returns user data fields of message.
    pub fn fields(&self) -> Option<MessageFields> {
        if self.fields.is_none() {
            return None;
        }

        self.fields.clone()
    }

    /// Returns true if message contains fields.
    pub fn has_fields(&self) -> bool {
        self.fields.is_some()
    }

    /// Returns an encoded version of this message.
    pub fn encode(&self) -> Result<MessageEncoded> {
        // Encode bytes as hex string
        let encoded = hex::encode(&self.as_cbor());

        Ok(MessageEncoded::new(&encoded)?)
    }
}

#[cfg(test)]
mod tests {
    use crate::atomic::Hash;

    use super::{Message, MessageFields, MessageValue};

    #[test]
    fn message_fields() {
        let mut fields = MessageFields::new();

        // Detect duplicate
        fields
            .add("test", MessageValue::Text("Hello, Panda!".to_owned()))
            .unwrap();

        assert!(fields
            .add("test", MessageValue::Text("Huhu".to_owned()))
            .is_err());

        // Bail when key does not exist
        assert!(fields
            .update("imagine", MessageValue::Text("Pandaparty".to_owned()))
            .is_err());
    }

    #[test]
    fn encode_and_decode() {
        // Create test message
        let mut fields = MessageFields::new();

        fields
            .add("test", MessageValue::Text("Hello, Message!".to_owned()))
            .unwrap();

        let message = Message::update(
            Hash::from_bytes(vec![1, 255, 0]).unwrap(),
            Hash::from_bytes(vec![62, 128]).unwrap(),
            fields,
        )
        .unwrap();

        assert!(message.is_update());

        // Encode message ...
        let encoded = message.encode().unwrap();

        // ... and decode it again
        let message_restored = Message::from_encoded(encoded);

        assert_eq!(message, message_restored);
    }
}
