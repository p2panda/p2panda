
use std::convert::TryInto;
use serde::Serialize;

use p2panda_rs::entry::{LogId, SeqNum};
use p2panda_rs::message::Message;
use p2panda_rs::hash::Hash;

use crate::client::Client;
use crate::node::Node;

// A custom `Result` type to be able to dynamically propagate `Error` types.
pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(Serialize)]
pub struct NextEntryArgs {
    pub backlink: Option<Hash>,
    pub skiplink: Option<Hash>,
    pub seq_num: SeqNum,
    pub log_id: LogId,
}

/// Helper method signing and encoding entry and sending it to node backend.
pub fn send_to_node(
    node: &mut Node,
    client: &Client,
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
