// SPDX-License-Identifier: AGPL-3.0-or-later
pub mod utils;
pub mod logs;
pub mod node;
pub mod materializer;
pub mod test_data;

use std::collections::HashMap;
use std::convert::TryFrom;

use p2panda_rs::entry::{decode_entry, sign_and_encode, Entry, EntrySigned, LogId, SeqNum};
use p2panda_rs::identity::{Author, KeyPair};
use p2panda_rs::message::{Message, MessageEncoded};

use crate::utils::{calculate_entry_args, NextEntryArgs};

// const META_SCHEMA: &str  = "004069db5208a271c53de8a1b6220e6a4d7fcccd89e6c0c7e75c833e34dc68d932624f2ccf27513f42fb7d0e4390a99b225bad41ba14a6297537246dbe4e6ce150e8";

pub type TestPandaDB = HashMap<Author, Panda>;
pub type Logs = HashMap<usize, Vec<(EntrySigned, MessageEncoded)>>;
/// A helper struct for creating entries and performing psuedo log actions:
/// - publish create, update and delete messages to a schema log
/// - schema logs are stored on a Panda (Author) instance
/// - static helper methods for creating entries, messages etc....
#[derive(Debug)]
pub struct Panda {
    pub name: String,
    pub key_pair: KeyPair,
}

impl Panda {
    pub fn new(name: String, key_pair: KeyPair) -> Self {
        Self {
            name,
            key_pair,
        }
    }
    
    pub fn author(&self) -> Author {
        Author::new(&self.public_key()).unwrap()
    }

    pub fn private_key(&self) -> String {
        hex::encode(self.key_pair.private_key())
    }

    pub fn public_key(&self) -> String {
        hex::encode(self.key_pair.public_key())
    }

    pub fn name(&self) -> String {
        self.name.to_owned()
    }

    /// Calculate the next entry arguments for this log (log_id, seq_num, backlink, skiplink)
    // pub fn next_entry_args(&self, log_id: usize) -> NextEntryArgs {
    //     let schema_entries = self.logs.get(&log_id).unwrap();
    //     calculate_entry_args(log_id, schema_entries.to_owned())
    // }

    /// Calculate the next entry arguments *at a certain point* in this log. This is helpful
    /// when generating test data and wanting to test the flow from requesting entry args through
    /// to publishing an entry
    // pub fn next_entry_args_for_specific_entry(
    //     &self,
    //     log_id: usize,
    //     seq_num: &SeqNum,
    // ) -> NextEntryArgs {
    //     let schema_entries = self.logs.get(&log_id).unwrap();
    //     calculate_entry_args(
    //         log_id,
    //         schema_entries[..seq_num.as_i64() as usize - 1].to_owned(),
    //     )
    // }
    
    /// Publish an entry to a schema log for this Panda
    pub fn signed_encoded_entry(&self, message: Message, entry_args: NextEntryArgs) -> EntrySigned {

        // Construct entry from message and entry args then sign and encode it
        let entry = Entry::new(
            &entry_args.log_id,
            Some(&message),
            entry_args.skiplink.as_ref(),
            entry_args.backlink.as_ref(),
            &entry_args.seq_num,
        )
        .unwrap();

        let entry_encoded = sign_and_encode(&entry, &self.key_pair).unwrap();

        entry_encoded
    }

    // pub fn get_entry(&self, schema: &str, seq_num: usize) -> Entry {
    //     let log_id = self.schema.get(schema).unwrap();
    //     let entry = &self.logs.get(&log_id).unwrap()[seq_num - 1];
    //     decode_entry(&entry.0, Some(&entry.1)).unwrap()
    // }

    // pub fn get_encoded_entry_and_message(
    //     &self,
    //     schema: &str,
    //     seq_num: usize,
    // ) -> (EntrySigned, MessageEncoded) {
    //     let log_id = self.schema.get(schema).unwrap();
    //     self.logs.get(log_id).unwrap()[seq_num - 1].clone()
    // }
}
