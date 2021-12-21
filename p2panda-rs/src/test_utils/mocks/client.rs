// SPDX-License-Identifier: AGPL-3.0-or-later

//! Mock p2panda client.
//!
//! This client mocks functionality which would be implemented in a real world p2panda client.
//! It does so in a simplistic manner and should only be used in a testing or demo
//! environment.
//!
//! ## Example
//! ```
//! # extern crate p2panda_rs;
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! use p2panda_rs::test_utils::mocks::{Client, send_to_node, Node};
//! use p2panda_rs::test_utils::utils::{create_operation, hash, operation_fields,
//!     new_key_pair, update_operation
//! };
//! use p2panda_rs::operation::AsOperation;
//! use p2panda_rs::test_utils::constants::DEFAULT_SCHEMA_HASH;
//! use p2panda_rs::operation::OperationValue;
//!
//! # const CHAT_SCHEMA_HASH: &str = DEFAULT_SCHEMA_HASH;
//!
//! // Instantiate a new mock node
//! let mut node = Node::new();
//!
//! // Instantiate one client named "panda"
//! let panda = Client::new("panda".to_string(), new_key_pair());
//!
//! // Create a new operation to publish
//! let operation = create_operation(
//!     hash(DEFAULT_SCHEMA_HASH),
//!     operation_fields(vec![("message", OperationValue::Text("Ohh, my first message!".to_string()))]),
//! );
//!
//! // Retrieve the next entry args from the node
//! let entry_args = node.next_entry_args(&panda.author(), &operation.schema(), None)?;
//!
//! // Sign and encode an entry
//! let entry_encoded = panda.signed_encoded_entry(operation.to_owned(), entry_args);
//! node.publish_entry(&entry_encoded, &operation)?;
//!
//! # Ok(())
//! # }
//! ```

use crate::entry::{sign_and_encode, Entry, EntrySigned};
use crate::identity::{Author, KeyPair};
use crate::operation::Operation;

use crate::test_utils::utils::NextEntryArgs;

/// A helper struct which represents a client in the pandaverse. It is a thin wrapper around a KeyPair, it is used when creating, signing and publishing entries.
#[derive(Debug)]
pub struct Client {
    /// Name of this client
    pub name: String,
    /// The keypair of this client
    pub key_pair: KeyPair,
}

impl Client {
    /// Create a new client passing in name and key_pair
    pub fn new(name: String, key_pair: KeyPair) -> Self {
        Self { name, key_pair }
    }

    /// Get an author instance for this client
    pub fn author(&self) -> Author {
        Author::new(&self.public_key()).unwrap()
    }

    /// Get the private key for this author
    pub fn private_key(&self) -> String {
        hex::encode(self.key_pair.private_key())
    }

    /// Get the public key identifier for this author
    pub fn public_key(&self) -> String {
        hex::encode(self.key_pair.public_key())
    }

    /// Get the name of this author
    pub fn name(&self) -> String {
        self.name.to_owned()
    }

    /// Create, sign and encode an entry
    pub fn signed_encoded_entry(
        &self,
        operation: Operation,
        entry_args: NextEntryArgs,
    ) -> EntrySigned {
        // Construct entry from operation and entry args then sign and encode it
        let entry = Entry::new(
            &entry_args.log_id,
            Some(&operation),
            entry_args.skiplink.as_ref(),
            entry_args.backlink.as_ref(),
            &entry_args.seq_num,
        )
        .unwrap();

        sign_and_encode(&entry, &self.key_pair).unwrap()
    }
}
