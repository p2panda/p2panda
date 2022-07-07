// SPDX-License-Identifier: AGPL-3.0-or-later

//! Mock p2panda client.
//!
//! This client mocks functionality which would be implemented in a real world p2panda client. It
//! does so in a simplistic manner and should only be used in a testing or demo environment.
//!
//! ## Example
//!
//! ```
//! # extern crate p2panda_rs;
//! # #[tokio::main]
//! # async fn main() -> p2panda_rs::storage_provider::utils::Result<()> {
//! use std::convert::TryFrom;
//!
//! use p2panda_rs::operation::AsOperation;
//! use p2panda_rs::operation::{OperationEncoded, OperationValue};
//! use p2panda_rs::test_utils::constants::SCHEMA_ID;
//! use p2panda_rs::test_utils::mocks::{send_to_node, Client, Node};
//! use p2panda_rs::test_utils::fixtures::{
//!     create_operation, schema, random_key_pair, operation_fields
//! };
//!
//! // Instantiate a new mock node
//! let mut node = Node::new();
//!
//! // Instantiate one client named "panda"
//! let panda = Client::new("panda".to_string(), random_key_pair());
//!
//! // Create a new operation to publish
//! let operation = create_operation(
//!     &[(
//!         "message",
//!         OperationValue::Text("Ohh, my first message!".to_string()),
//!     )],
//! );
//!
//! println!("{:#?}", operation);
//!
//! // Retrieve the next entry args from the node
//! let args = node.get_next_entry_args(&panda.author(), None).await?;
//!
//! // Sign and encode an entry
//! let entry_encoded = panda.signed_encoded_entry(
//!     operation.to_owned(),
//!     &args.log_id,
//!     args.skiplink.as_ref(),
//!     args.backlink.as_ref(),
//!     &args.seq_num
//! );
//! let operation_encoded = OperationEncoded::try_from(&operation)?;
//!
//! node.publish_entry(&entry_encoded, &operation_encoded).await?;
//!
//! # Ok(())
//! # }
//! ```
use crate::entry::{sign_and_encode, Entry, EntrySigned, LogId, SeqNum};
use crate::hash::Hash;
use crate::identity::{Author, KeyPair};
use crate::operation::Operation;

/// A helper struct which represents a client in the pandaverse.
///
/// It is a thin wrapper around a [`KeyPair`], it is used when creating, signing and publishing
/// entries.
#[derive(Debug)]
pub struct Client {
    /// Name of this client.
    pub name: String,

    /// The key pair of this client.
    pub key_pair: KeyPair,
}

impl Client {
    /// Create a new client passing in a name and key pair.
    pub fn new(name: String, key_pair: KeyPair) -> Self {
        Self { name, key_pair }
    }

    /// Returns author of this client.
    pub fn author(&self) -> Author {
        Author::new(&self.public_key()).unwrap()
    }

    /// Get the private key for this author.
    pub fn private_key(&self) -> String {
        hex::encode(self.key_pair.private_key())
    }

    /// Get the public key encoded as a hex string for this author.
    pub fn public_key(&self) -> String {
        hex::encode(self.key_pair.public_key())
    }

    /// Get the name of this author.
    pub fn name(&self) -> String {
        self.name.to_owned()
    }

    /// Create, sign and encode an entry.
    pub fn signed_encoded_entry(
        &self,
        operation: Operation,
        log_id: &LogId,
        skiplink: Option<&Hash>,
        backlink: Option<&Hash>,
        seq_num: &SeqNum,
    ) -> EntrySigned {
        let entry = Entry::new(log_id, Some(&operation), skiplink, backlink, seq_num).unwrap();

        sign_and_encode(&entry, &self.key_pair).unwrap()
    }
}
