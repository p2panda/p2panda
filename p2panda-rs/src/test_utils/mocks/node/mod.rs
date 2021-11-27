// SPDX-License-Identifier: AGPL-3.0-or-later

//! Mock p2panda node, related data types and utilities.
//!
//! This node mocks functionality which would be implemented in a real world p2panda node.
//! It does so in a simplistic manner and should only be used in a testing environment or demo
//! environment.
//!
//! ## Example
//! ```
//! use p2panda_rs::test_utils::mocks::client::Client;
//! use p2panda_rs::test_utils::mocks::node::{send_to_node, Node};
//! use p2panda_rs::test_utils::{create_message, delete_message, hash, message_fields, 
//!     new_key_pair, update_message, DEFAULT_SCHEMA_HASH
//! };
//! # const CHAT_SCHEMA_HASH: &str = DEFAULT_SCHEMA_HASH;
//!
//! // Instantiate a new mock node
//! let mut node = Node::new();
//!
//! // Instantiate one client named "panda"
//! let panda = Client::new("panda".to_string(), new_key_pair());
//!
//! // Panda creates a chat message by publishing a CREATE operation
//! let instance_a_hash = send_to_node(
//!     &mut node,
//!     &panda,
//!     &create_message(
//!         hash(CHAT_SCHEMA_HASH),
//!         message_fields(vec![("message", "Ohh, my first message!")]),
//!     ),
//! )
//! .unwrap();
//!
//! // Panda updates their previous chat message by publishing an UPDATE operation
//! send_to_node(
//!     &mut node,
//!     &panda,
//!     &update_message(
//!         hash(CHAT_SCHEMA_HASH),
//!         instance_a_hash.clone(),
//!         message_fields(vec![("message", "Which I now update.")]),
//!     ),
//! )
//! .unwrap();
//!
//! // Panda deletes their previous chat message by publishing a DELETE operation
//! send_to_node(
//!     &mut node,
//!     &panda,
//!     &delete_message(hash(CHAT_SCHEMA_HASH), instance_a_hash),
//! )
//! .unwrap();
//!
//! // Panda creates another chat message by publishing a CREATE operation
//! send_to_node(
//!     &mut node,
//!     &panda,
//!     &create_message(
//!         hash(CHAT_SCHEMA_HASH),
//!         message_fields(vec![("message", "Let's try that again.")]),
//!     ),
//! )
//! .unwrap();
//!
//! // Get all entries published to this node
//! let entries = node.all_entries();
//!
//! // There should be 4 entries
//! entries.len(); // => 4
//!
//! // Query all instances of a certain schema
//! let instances = node.query_all(&CHAT_SCHEMA_HASH.to_string()).unwrap();
//!
//! // There should be one instance, because on was deleted
//! instances.len(); // => 1
//!
//! // Query for one instance by id
//! let instance = node
//!     .query(&CHAT_SCHEMA_HASH.to_string(), &entries[3].hash_str())
//!     .unwrap();
//!
//! instance.get("message").unwrap(); // => "Let's try that again."
//!
//! ```

mod node;
pub mod utils;

pub use node::{send_to_node, Database, Node};
