use std::collections::btree_map::Iter;
use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_repr::{Deserialize_repr, Serialize_repr};

use crate::hash::Hash;
use crate::message::{MessageEncoded, MessageError, MessageFieldsError};
use crate::Validate;

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
/// An action defines the message format and if this message creates, updates or deletes a data
/// instance.
#[derive(Clone, Debug, PartialEq)]
pub enum MessageAction {
    /// Message creates a new data instance.
    Create,

    /// Message updates an existing data instance.
    Update,

    /// Message deletes an existing data instance.
    Delete,
}

impl Serialize for MessageAction {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
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
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
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
#[serde(tag = "type", content = "value")]
pub enum MessageValue {
    /// Basic `boolean` value.
    #[serde(rename = "bool")]
    Boolean(bool),

    /// Basic signed `integer` value.
    #[serde(rename = "int")]
    Integer(i64),

    /// Basic signed `float` value.
    #[serde(rename = "float")]
    Float(f64),

    /// Basic `string` value.
    #[serde(rename = "str")]
    Text(String),

    /// Reference to an instance.
    #[serde(rename = "relation")]
    Relation(Hash),
}

/// Message fields are used to store user data. They are implemented as a simple key/value store
/// with support for a limited number of data types (see [`MessageValue`] for further documentation
/// on this). A `MessageFields` instance can contain any number and types of fields. However, when
/// a `MessageFields` instance is attached to a `Message`, the message's schema determines which
/// fields may be used.
///
/// Internally message fields use sorted B-Tree maps to assure ordering of the fields. If the
/// message fields would not be sorted consistently we would get different hash results for the
/// same contents.
///
/// # Example
///
/// ```
/// # extern crate p2panda_rs;
/// # fn main() -> () {
/// # use p2panda_rs::message::{MessageFields, MessageValue};
/// let mut fields = MessageFields::new();
/// fields
///     .add("title", MessageValue::Text("Hello, Panda!".to_owned()))
///     .unwrap();
/// }
/// ```
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MessageFields(BTreeMap<String, MessageValue>);

impl MessageFields {
    /// Creates a new fields instance to add data to.
    pub fn new() -> Self {
        Self(BTreeMap::new())
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
    pub fn add(&mut self, name: &str, value: MessageValue) -> Result<(), MessageFieldsError> {
        if self.0.contains_key(name) {
            return Err(MessageFieldsError::FieldDuplicate);
        }

        self.0.insert(name.to_owned(), value);

        Ok(())
    }

    /// Overwrites an already existing field with a new value.
    pub fn update(&mut self, name: &str, value: MessageValue) -> Result<(), MessageFieldsError> {
        if !self.0.contains_key(name) {
            return Err(MessageFieldsError::UnknownField);
        }

        self.0.insert(name.to_owned(), value);

        Ok(())
    }

    /// Removes an existing field from this instance.
    pub fn remove(&mut self, name: &str) -> Result<(), MessageFieldsError> {
        if !self.0.contains_key(name) {
            return Err(MessageFieldsError::UnknownField);
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

    /// Returns an array of existing message keys.
    pub fn keys(&self) -> Vec<String> {
        self.0.keys().cloned().collect()
    }

    /// Returns an iterator of existing message fields.
    pub fn iter(&self) -> Iter<String, MessageValue> {
        self.0.iter()
    }
}

/// Messages describe data mutations in the p2panda network. Authors send messages to create,
/// update or delete instances or collections of data.
///
/// The data itself lives in the `fields` object and is formed after a message schema.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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

impl Message {
    /// Returns new create message.
    ///
    /// ## Example
    ///
    /// ```
    /// # extern crate p2panda_rs;
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// use p2panda_rs::hash::Hash;
    /// use p2panda_rs::message::{Message, MessageFields, MessageValue};
    ///
    /// let schema_hash_string = "004069db5208a271c53de8a1b6220e6a4d7fcccd89e6c0c7e75c833e34dc68d932624f2ccf27513f42fb7d0e4390a99b225bad41ba14a6297537246dbe4e6ce150e8";
    /// let schema_msg_hash = Hash::new(schema_hash_string)?;
    /// let mut msg_fields = MessageFields::new();
    ///
    /// msg_fields
    ///     .add("Zoo", MessageValue::Text("Pandas, Doggos, Cats, and Parrots!".to_owned()))
    ///     .unwrap();
    ///
    /// let create_message = Message::new_create(schema_msg_hash, msg_fields)?;
    ///
    /// assert_eq!(Message::is_create(&create_message), true);
    ///
    /// # Ok(())
    /// # }
    /// ```
    pub fn new_create(schema: Hash, fields: MessageFields) -> Result<Self, MessageError> {
        let message = Self {
            action: MessageAction::Create,
            version: MessageVersion::Default,
            schema,
            id: None,
            fields: Some(fields),
        };

        message.validate()?;

        Ok(message)
    }

    /// Returns new update message.
    pub fn new_update(schema: Hash, id: Hash, fields: MessageFields) -> Result<Self, MessageError> {
        let message = Self {
            action: MessageAction::Update,
            version: MessageVersion::Default,
            schema,
            id: Some(id),
            fields: Some(fields),
        };

        message.validate()?;

        Ok(message)
    }

    /// Returns new delete message.
    pub fn new_delete(schema: Hash, id: Hash) -> Result<Self, MessageError> {
        let message = Self {
            action: MessageAction::Delete,
            version: MessageVersion::Default,
            schema,
            id: Some(id),
            fields: None,
        };

        message.validate()?;

        Ok(message)
    }

    /// Encodes message in CBOR format and returns bytes.
    pub fn to_cbor(&self) -> Vec<u8> {
        // Serialize data to binary CBOR format
        serde_cbor::to_vec(&self).unwrap()
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

    /// Returns action type of message.
    pub fn action(&self) -> &MessageAction {
        &self.action
    }

    /// Returns version of message.
    pub fn version(&self) -> &MessageVersion {
        &self.version
    }

    /// Returns schema of message.
    pub fn schema(&self) -> &Hash {
        &self.schema
    }

    /// Returns id of message.
    pub fn id(&self) -> Option<&Hash> {
        self.id.as_ref()
    }

    /// Returns user data fields of message.
    pub fn fields(&self) -> Option<&MessageFields> {
        self.fields.as_ref()
    }

    /// Returns true when message contains an id.
    pub fn has_id(&self) -> bool {
        self.id.is_some()
    }

    /// Returns true if message contains fields.
    pub fn has_fields(&self) -> bool {
        self.fields.is_some()
    }
}

/// Decodes an encoded message and returns it.
impl From<&MessageEncoded> for Message {
    fn from(message_encoded: &MessageEncoded) -> Self {
        serde_cbor::from_slice(&message_encoded.to_bytes()).unwrap()
    }
}

impl Validate for Message {
    type Error = MessageError;

    fn validate(&self) -> Result<(), Self::Error> {
        // Create and update messages can not have empty fields.
        if !self.is_delete() && (!self.has_fields() || self.fields().unwrap().is_empty()) {
            return Err(MessageError::EmptyFields);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::convert::TryFrom;

    use crate::hash::Hash;
    use crate::message::MessageEncoded;

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

        // Add one field for every kind of MessageValue
        fields
            .add("username", MessageValue::Text("bubu".to_owned()))
            .unwrap();

        fields.add("height", MessageValue::Float(3.5)).unwrap();

        fields.add("age", MessageValue::Integer(28)).unwrap();

        fields
            .add("is_admin", MessageValue::Boolean(false))
            .unwrap();

        fields
            .add(
                "profile_picture",
                MessageValue::Relation(Hash::new_from_bytes(vec![1, 2, 3]).unwrap()),
            )
            .unwrap();

        let message = Message::new_update(
            Hash::new_from_bytes(vec![1, 255, 0]).unwrap(),
            Hash::new_from_bytes(vec![62, 128]).unwrap(),
            fields,
        )
        .unwrap();

        assert!(message.is_update());

        // Encode message ...
        let encoded = MessageEncoded::try_from(&message).unwrap();

        // ... and decode it again
        let message_restored = Message::try_from(&encoded).unwrap();

        assert_eq!(message, message_restored);
    }

    #[test]
    fn field_ordering() {
        // Create first test message
        let mut fields = MessageFields::new();
        fields
            .add("a", MessageValue::Text("sloth".to_owned()))
            .unwrap();
        fields
            .add("b", MessageValue::Text("penguin".to_owned()))
            .unwrap();

        let first_message =
            Message::new_create(Hash::new_from_bytes(vec![1, 255, 0]).unwrap(), fields).unwrap();

        // Create second test message with same values but different order of fields
        let mut second_fields = MessageFields::new();
        second_fields
            .add("b", MessageValue::Text("penguin".to_owned()))
            .unwrap();
        second_fields
            .add("a", MessageValue::Text("sloth".to_owned()))
            .unwrap();

        let second_message = Message::new_create(
            Hash::new_from_bytes(vec![1, 255, 0]).unwrap(),
            second_fields,
        )
        .unwrap();

        assert_eq!(first_message.to_cbor(), second_message.to_cbor());
    }

    #[test]
    fn field_iteration() {
        // Create first test message
        let mut fields = MessageFields::new();
        fields
            .add("a", MessageValue::Text("sloth".to_owned()))
            .unwrap();
        fields
            .add("b", MessageValue::Text("penguin".to_owned()))
            .unwrap();

        let mut field_iterator = fields.iter();

        assert_eq!(
            field_iterator.next().unwrap().1,
            &MessageValue::Text("sloth".to_owned())
        );
        assert_eq!(
            field_iterator.next().unwrap().1,
            &MessageValue::Text("penguin".to_owned())
        );
    }
}
