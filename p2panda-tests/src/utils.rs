
use std::convert::TryInto;
use std::collections::HashMap;
use serde::Serialize;
use bamboo_rs_core::entry::is_lipmaa_required;

use p2panda_rs::entry::{decode_entry, EntrySigned, LogId, SeqNum};
use p2panda_rs::message::{Message, MessageEncoded};
use p2panda_rs::hash::Hash;

use crate::Panda;
use crate::node::Node;

// A custom `Result` type to be able to dynamically propagate `Error` types.
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(Serialize)]
#[allow(non_snake_case)]
pub struct NextEntryArgs {
    pub backlink: Option<Hash>,
    pub skiplink: Option<Hash>,
    pub seq_num: SeqNum,
    pub log_id: LogId,
}

pub fn calculate_entry_args(
    log_id: usize,
    schema_entries: Vec<(EntrySigned, MessageEncoded)>,
) -> NextEntryArgs {
    if schema_entries.len() == 0 {
        NextEntryArgs {
            backlink: None,
            skiplink: None,
            seq_num: SeqNum::new(1).unwrap(),
            log_id: LogId::new(log_id.try_into().unwrap()),
        }
    } else {
        // Get last entry in log
        let (entry_encoded, _) = schema_entries.get(schema_entries.len() - 1).unwrap();
        let decoded_last_entry = decode_entry(&entry_encoded, None).unwrap();

        // Get the hash (which is the backlink we need)
        let backlink = Some(entry_encoded.hash());

        // Get the next sequence number
        let next_seq_num = decoded_last_entry.seq_num().to_owned().next().unwrap();

        // And then the skiplink
        let skiplink_seq_num = next_seq_num.skiplink_seq_num().unwrap().as_i64();

        // And finally the log id
        let log_id = decoded_last_entry.log_id();

        // Check if skiplink is required and return hash if so
        let skiplink = if is_lipmaa_required(next_seq_num.as_i64() as u64) {
            let (skiplink_entry, _) = schema_entries.get(skiplink_seq_num as usize).unwrap();
            Some(skiplink_entry.hash())
        } else {
            None
        };

        NextEntryArgs {
            backlink,
            skiplink,
            seq_num: next_seq_num,
            log_id: log_id.to_owned(),
        }
    }
}

/// Helper method signing and encoding entry and sending it to node backend.
pub fn send_to_node(
    node: &mut Node,
    client: &Panda,
    message: &Message,
) -> Result<Hash> {
    let entry_args =
        node.next_entry_args(&client.author(), message.schema())?;

    let entry_encoded = client.signed_encoded_entry(
        message.to_owned(),
        entry_args, 
    );

    node.publish_entry(&entry_encoded, &message)?;

    // Return entry hash for now so we can use it to perform UPDATE and DELETE messages later
    Ok(entry_encoded.hash())
}
