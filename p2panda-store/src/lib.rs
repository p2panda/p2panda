// SPDX-License-Identifier: MIT OR Apache-2.0

#![cfg_attr(docsrs, feature(doc_cfg))]

//! Trait definitions and SQLite implementations for persistent stores used by p2panda.
//!
//! This crate provides generic trait definitions to flexibly express storage and query behaviour
//! for a wide-range of peer-to-peer systems. In the context of p2panda these include an address
//! book for managing transport information related to nodes in a network, an operation store for
//! maintaining append-only log entries, an orderer store to track operation dependencies, and much
//! more. Concrete SQLite database implementations are provided for all store traits, along with a
//! transaction provider for cases when atomicity and consistency are required for a set of database
//! interactions.
//!
//! ## Features
//!
//! - Generic trait definitions required to implement p2panda stores
//! - SQLite implementations for all p2panda stores
//!   - Address book for handling node information
//!   - Cursors for tracking positions in logs
//!   - Groups for maintaining auth group state
//!   - Logs for efficient comparison of log-based data types
//!   - Operations for storing entries in append-only logs
//!   - Orderer for tracking dependencies in partially-ordered data sets
//! - Transaction provider to group related queries for consistency guarantees
//! - Database migrations on store creation or during application runtime
//!
//! ## Examples
//!
//! ### Create an in-memory SQLite store
//!
//! ```rust
//! use p2panda_store::SqliteStoreBuilder;
//! # #[tokio::main(flavor = "current_thread")]
//! # async fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let store = SqliteStoreBuilder::new().build().await?;
//! # Ok(())
//! # }
//! ```
//!
//! ### Create and insert a new operation
//!
//! Here we use the `tx!` macro provided by `p2panda-store` to group several queries into a single
//! transaction.
//!
//! ```rust
//! # use p2panda_core::{Topic, Header, SeqNum, Hash, Body, Operation, VerifyingKey, SigningKey};
//! # use p2panda_store::logs::LogStore;
//! # use p2panda_store::operations::OperationStore;
//! # use p2panda_store::topics::TopicStore;
//! # use p2panda_store::{SqliteStore, tx};
//! #
//! # #[tokio::main(flavor = "current_thread")]
//! # async fn main() -> Result<(), Box<dyn std::error::Error>> {
//! #
//! # let store = SqliteStore::temporary().await;
//! #
//! # let log_id = 2;
//! # let topic = Topic::random();
//! # let signing_key = SigningKey::generate();
//! # let body = Body::new(b"Transaction! Yay!");
//! #
//! // Acquire a lock on the store for the duration of the read to write cycle.
//! //
//! // This is to ensure that the data returned from the `get_latest_entry()` query does not
//! // become stale before the call to `insert_operation()`.
//! //
//! // Here we acquire a store permit, query the latest log entry, associate the topic with
//! // the log, insert the operation and commit the transaction before dropping the permit.
//! let operation = tx!(store, {
//!     let (seq_num, backlink) = <SqliteStore as LogStore<
//!         Operation<()>,
//!         VerifyingKey,
//!         u64,
//!         SeqNum,
//!         Hash,
//!     >>::get_latest_entry_tx(
//!         &store, &signing_key.verifying_key(), &log_id
//!     )
//!     .await?
//!     .map(|operation| (operation.header.seq_num + 1, Some(operation.hash)))
//!     .unwrap_or((0, None));
//!
//!     let mut header = Header {
//!         version: 1,
//!         verifying_key: signing_key.verifying_key(),
//!         signature: None,
//!         payload_size: body.size(),
//!         payload_hash: Some(body.hash()),
//!         seq_num,
//!         backlink,
//!         extensions: (),
//!     };
//!
//!     header.sign(&signing_key);
//!     let hash = header.hash();
//!
//!     let operation = Operation {
//!         hash,
//!         header: header.clone(),
//!         body: Some(body),
//!     };
//!
//!     <SqliteStore as TopicStore<Topic, VerifyingKey, u64>>::associate(
//!         &store,
//!         &topic,
//!         &signing_key.verifying_key(),
//!         &log_id,
//!     )
//!     .await?;
//!
//!     store
//!         .insert_operation(&hash, &operation, &log_id)
//!         .await?;
//!
//!     operation
//! });
//! # Ok(())
//! # }
//! ```
pub mod address_book;
pub mod cursors;
pub mod groups;
pub mod key_registry;
pub mod key_secrets;
pub mod logs;
#[cfg(feature = "macros")]
mod macros;
pub mod operations;
pub mod orderer;
pub mod spaces;
#[cfg(feature = "sqlite")]
pub mod sqlite;
pub mod topics;
mod traits;

#[cfg(feature = "sqlite")]
#[doc(inline)]
pub use sqlite::{SqliteError, SqliteStore, SqliteStoreBuilder};
pub use traits::Transaction;
