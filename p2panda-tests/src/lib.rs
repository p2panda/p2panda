// SPDX-License-Identifier: AGPL-3.0-or-later
mod fixtures;
mod templates;
#[cfg(test)]
mod tests;
pub mod utils;

use serde_json;
use std::collections::HashMap;
use std::convert::TryFrom;

use p2panda_rs::entry::{decode_entry, sign_and_encode, Entry, EntrySigned, LogId, SeqNum};
use p2panda_rs::hash::Hash;
use p2panda_rs::identity::{Author, KeyPair};
use p2panda_rs::message::{Message, MessageEncoded, MessageFields, MessageValue};

use bamboo_rs_core::entry::is_lipmaa_required;
use rstest_reuse;

// const META_SCHEMA: &str  = "004069db5208a271c53de8a1b6220e6a4d7fcccd89e6c0c7e75c833e34dc68d932624f2ccf27513f42fb7d0e4390a99b225bad41ba14a6297537246dbe4e6ce150e8";

pub type TestPandaDB = HashMap<Author, Panda>;
pub type Logs = HashMap<usize, Vec<(EntrySigned, MessageEncoded)>>;

#[derive(Debug)]
pub struct Panda {
    pub name: String,
    pub schema: HashMap<String, usize>,
    pub key_pair: KeyPair,
    pub logs: Logs,
}

/// A helper struct for creating entries and performing psuedo log actions:
/// - publish create, update and delete messages to a schema log
/// - schema logs are stored on a Panda (Author) instance
/// - static helper methods for creating entries, messages etc....
impl Panda {
    pub fn new(name: String, key_pair: KeyPair) -> Self {
        Self {
            name,
            schema: HashMap::new(),
            key_pair,
            logs: HashMap::new(),
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

    pub fn message(
        schema: &str,
        instance_id: Option<Hash>,
        fields: Option<Vec<(&str, &str)>>,
    ) -> Message {
        match fields {
            // It's a CREATE message
            Some(fields) if instance_id.is_none() => Panda::create_message(schema, fields),
            // It's an UPDATE message
            Some(fields) => Panda::update_message(schema, instance_id.unwrap(), fields),
            // It's a DELETE message
            None => Panda::delete_message(schema, instance_id.unwrap()),
        }
    }

    pub fn create_message(schema: &str, fields: Vec<(&str, &str)>) -> Message {
        let fields = Panda::build_message_fields(fields);
        Message::new_create(Hash::new(schema).unwrap(), fields).unwrap()
    }

    pub fn update_message(schema: &str, instance_id: Hash, fields: Vec<(&str, &str)>) -> Message {
        let fields = Panda::build_message_fields(fields);
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

    pub fn some_hash(str: &str) -> Option<Hash> {
        let hash = Hash::new(str);
        Some(hash.unwrap())
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
    
    pub fn name(&self) -> String {
        self.name.to_owned()
    }
    
    fn get_schema_id(&self, schema: Hash) -> usize {
        self.schema.get(schema.as_str()).unwrap().to_owned()
    }

    /// Determine the skiplink for the next entry
    fn next_entry_args(&mut self, log_id: usize) -> (SeqNum, Option<Hash>, Option<Hash>) {

        let schema_entries = self.logs.get(&log_id).unwrap();

        if schema_entries.len() == 0 {
            (Panda::seq_num(1), None, None)
        } else {
            // Get last entry in log
            let (entry_encoded, message_encoded) =
                schema_entries.get(schema_entries.len() - 1).unwrap();
            let decoded_last_entry = decode_entry(&entry_encoded, None).unwrap();

            // Get the hash (which is the backlink we need)
            let backlink = Some(entry_encoded.hash());

            // Get the next sequence number
            let next_seq_num = decoded_last_entry.seq_num().to_owned().next().unwrap();

            // And then the skiplink
            let skiplink_seq_num = next_seq_num.skiplink_seq_num().unwrap().as_i64();

            // Check if skiplink is required and return hash if so
            let skiplink = if is_lipmaa_required(next_seq_num.as_i64() as u64) {
                let (skiplink_entry, _) = schema_entries.get(skiplink_seq_num as usize).unwrap();
                Some(skiplink_entry.hash())
            } else {
                None
            };

            // Return next entry args
            (next_seq_num, backlink, skiplink)
        }
    }

    /// Publish an entry to a schema log for this Panda
    pub fn publish_entry(&mut self, message: Message) {
        
        let schema_str = message.schema().as_str();

        let log_id = match self.schema.get(schema_str) {
            Some(id) => id.to_owned(),
            None => {
                let id = self.logs.len() + 1;
                self.schema.insert(schema_str.to_string(), id);
                self.logs.insert(id, Vec::new());
                id
            }
        };
        
        // Calculate next entry args
        let (seq_num, backlink, skiplink) = self.next_entry_args(log_id);

        // Construct entry from message and entry args then sign and encode it
        let entry = Panda::entry(&message, &seq_num, backlink, skiplink);
        let entry_encoded = sign_and_encode(&entry, &self.key_pair).unwrap();

        // Encode message
        let message_encoded = MessageEncoded::try_from(&message).unwrap();

        // Push new entry to schema log
        let schema_entries = self.logs.get_mut(&log_id).unwrap();
        schema_entries.push((entry_encoded, message_encoded));
    }

    pub fn get_entry(&self, schema: &str, seq_num: usize) -> Entry {
        let log_id = self.schema.get(schema).unwrap();
        let entry = &self.logs.get(&log_id).unwrap()[seq_num - 1];
        decode_entry(&entry.0, Some(&entry.1)).unwrap()
    }

    pub fn get_encoded_entry_and_message(
        &self,
        schema: &str,
        seq_num: usize,
    ) -> (EntrySigned, MessageEncoded) {
        let log_id = self.schema.get(schema).unwrap();
        self.logs.get(log_id).unwrap()[seq_num - 1].clone()
    }

    pub fn decode(&self) -> HashMap<usize, Vec<Entry>> {
        let mut decoded_logs: HashMap<usize, Vec<Entry>> = HashMap::new();
        for (hash, entries) in self.logs.iter() {
            decoded_logs.insert(hash.to_owned(), Vec::new());
            let log_entries = decoded_logs.get_mut(hash).unwrap();
            for (entry_encoded, message_encoded) in entries.iter() {
                let entry = decode_entry(entry_encoded, Some(message_encoded)).unwrap();
                log_entries.push(entry);
            }
        }
        decoded_logs
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(&self.logs).unwrap()
    }

    pub fn to_json_decoded(&self) -> String {
        serde_json::to_string_pretty(&self.decode()).unwrap()
    }
}
