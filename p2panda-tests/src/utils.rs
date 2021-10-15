// SPDX-License-Identifier: AGPL-3.0-or-later

use bamboo_rs_core::entry::is_lipmaa_required;
use std::collections::HashMap;
use std::convert::TryFrom;

use p2panda_rs::entry::{decode_entry, sign_and_encode, Entry, EntrySigned, LogId, SeqNum};
use p2panda_rs::hash::Hash;
use p2panda_rs::identity::{Author, KeyPair};
use p2panda_rs::message::{Message, MessageEncoded, MessageFields, MessageValue};

const META_SCHEMA: &str  = "004069db5208a271c53de8a1b6220e6a4d7fcccd89e6c0c7e75c833e34dc68d932624f2ccf27513f42fb7d0e4390a99b225bad41ba14a6297537246dbe4e6ce150e8";

pub type TestPandaDB = HashMap<Author, TestPanda>;

pub struct EntryData {
    pub entry_encoded: EntrySigned,
    pub message_encoded: MessageEncoded,
}
pub struct TestPanda {
    pub key_pair: KeyPair,
    pub logs: HashMap<String, Vec<EntryData>>,
}

/// A helper struct for creating entries and performing psuedo log actions:
/// - publish create, update and delete messages to a schema log
/// - schema logs are stored on a TestPanda (Author) instance
/// - static helper methods for creating entries, messages etc....
impl TestPanda {
    pub fn new(key_pair: KeyPair) -> Self {
        Self {
            key_pair,
            logs: HashMap::new(),
        }
    }

    /// Publish an entry to a schema log for this TestPanda
    pub fn publish_entry(&mut self, message: Message) {
        // Calculate next entry args
        let (seq_num, backlink, skiplink) = self.next_entry_args(message.schema().to_owned());

        // Construct entry from message and entry args then sign and encode it
        let entry = TestPanda::entry(&message, &seq_num, backlink, skiplink);
        let entry_encoded = sign_and_encode(&entry, &self.key_pair).unwrap();

        // Encode message
        let message_encoded = MessageEncoded::try_from(&message).unwrap();
        let schema_str: String = message.schema().as_str().into();

        // Push new entry to schema log
        let schema_entries = self.logs.get_mut(&schema_str).unwrap();
        schema_entries.push(EntryData {
            entry_encoded,
            message_encoded,
        });
    }

    /// Publish an entry to a schema log for this TestPanda
    pub fn build_message_fields(fields: Vec<(&str, &str)>) -> MessageFields {
        let mut message_fields = MessageFields::new();
        for (key, value) in fields.iter() {
            message_fields
                .add(
                    key,
                    MessageValue::Text(value.to_string()),
                )
                .unwrap();
        }
        message_fields
    }

    /// Determine the skiplink for the next entry
    fn next_entry_args(&mut self, schema: Hash) -> (SeqNum, Option<Hash>, Option<Hash>) {
        let schema_str: String = schema.as_str().into();

        // Retrieve schema log
        let schema_entries = self.logs.get(&schema_str);

        // Calculate next entry args
        match schema_entries {
            // If schema log doesn't exist create it and set entry args to first entry
            None => {
                self.logs.insert(schema_str, Vec::new());
                (TestPanda::seq_num(1), None, None)
            }
            // If schema log does exist calculate next entry args
            Some(schema_entries) => {
                // Get last entry in log
                let last_entry = schema_entries.get(schema_entries.len()).unwrap();
                let decoded_last_entry = decode_entry(&last_entry.entry_encoded, None).unwrap();

                // Get the hash (which is the backlink we need)
                let backlink = Some(last_entry.entry_encoded.hash());

                // Get the next sequence number
                let next_seq_num = decoded_last_entry.seq_num().to_owned().next().unwrap();

                // And then the skiplink
                let skiplink_seq_num = next_seq_num.skiplink_seq_num().unwrap().as_i64();

                // Check if skiplink is required and return hash if so
                let skiplink = if is_lipmaa_required(next_seq_num.as_i64() as u64) {
                    let skiplink_entry = schema_entries.get(skiplink_seq_num as usize).unwrap();
                    Some(skiplink_entry.entry_encoded.hash())
                } else {
                    None
                };

                // Return next entry args
                (next_seq_num, backlink, skiplink)
            }
        }
    }

    pub fn entry(
        message: &Message,
        seq_num: &SeqNum,
        backlink: Option<Hash>,
        skiplink: Option<Hash>,
    ) -> Entry {
        Entry::new(
            &LogId::default(),
            Some(message),
            skiplink.as_ref(),
            backlink.as_ref(),
            seq_num,
        )
        .unwrap()
    }

    pub fn create_message(schema: &str, fields: Vec<(&str, &str)>) -> Message {
        let fields = TestPanda::build_message_fields(fields);
        Message::new_create(Hash::new(schema).unwrap(), fields).unwrap()
    }

    pub fn update_message(schema: &str, instance_id: Hash, fields: Vec<(&str, &str)>) -> Message {
        let fields = TestPanda::build_message_fields(fields);
        Message::new_update(Hash::new(schema).unwrap(), instance_id, fields).unwrap()
    }

    pub fn delete_message(schema: &str, instance_id: Hash) -> Message {
        Message::new_delete(Hash::new(schema).unwrap(), instance_id).unwrap()
    }

    pub fn seq_num(seq_num: i64) -> SeqNum {
        SeqNum::new(seq_num).unwrap()
    }

    pub fn keypair_from_private(private_key: &str) -> KeyPair {
        KeyPair::from_private_key_str(private_key).unwrap()
    }

    pub fn keypair() -> KeyPair {
        KeyPair::new()
    }
}
