// SPDX-License-Identifier: MIT OR Apache-2.0

#![cfg_attr(doctest, doc=include_str!("../README.md"))]

//! Interfaces and implementations of persistence layers for p2panda data types and application
//! states.
//!
//! p2panda follows a strict separation of read- and write-only database interfaces to allow
//! designing efficient and fail-safe [atomic
//! transactions](https://youtu.be/5ZjhNTM8XU8?feature=shared&t=420) throughout the stack.
//!
//! ## Read queries
//!
//! `p2panda-store` currently offers all read-only trait interfaces for commonly used p2panda core
//! data-types and flows (for example "get the latest operation for this log"). These persistence
//! and query APIs are utilised by higher-level components of the p2panda stack, such as
//! `p2panda-sync` and `p2panda-stream`.
//!
//! For detailed information concerning the `Operation` type, please consult the documentation for
//! the `p2panda-core` crate.
//!
//! Logs in the context of `p2panda-store` are simply a collection of operations grouped under a
//! common identifier. The precise type for which `LogId` is implemented is left up to the
//! developer to decide according to their needs. With this in mind, the traits and implementations
//! provided by `p2panda-store` do not perform any validation of log integrity. Developers using
//! this crate must take steps to ensure their log design is fit for purpose and that all
//! operations have been thoroughly validated before being persisted.
//!
//! Also note that the traits provided here are not intended to offer generic storage solutions for
//! non-p2panda data types, nor are they intended to solve all application-layer storage concerns.
//!
//! ## Write transactions
//!
//! Multiple writes to a database should be grouped into one single, atomic transaction when they
//! need to strictly _all_ occur or _none_ occur. This is crucial to guarantee a crash-resiliant
//! p2p application, as any form of failure and disruption (user moving mobile app into the
//! background, etc.) might otherwise result in invalid database state which is hard to recover
//! from.
//!
//! `p2panda-store` offers `WritableStore`, `Transaction` and `WriteToStore` traits to accommodate
//! for exactly such a system and all p2panda implementations strictly follow the same pattern.
//!
//! ```rust
//! # use p2panda_store::{Transaction, WritableStore, WriteToStore};
//! #
//! # pub struct SqliteTransaction;
//! #
//! # impl Transaction for SqliteTransaction {
//! #     type Error = ();
//! #
//! #     fn commit(self) -> impl Future<Output = Result<(), Self::Error>> {
//! #         async { todo!() }
//! #     }
//! #
//! #     fn rollback(self) -> impl Future<Output = Result<(), Self::Error>> {
//! #         async { todo!() }
//! #     }
//! # }
//! #
//! # pub struct Sqlite;
//! #
//! # impl Sqlite {
//! #     pub fn new() -> Self {
//! #         Self
//! #     }
//! # }
//! #
//! # impl WritableStore for Sqlite {
//! #     type Error = ();
//! #
//! #     type Transaction<'c> = SqliteTransaction;
//! #
//! #     fn begin<'c>(
//! #         &mut self,
//! #     ) -> impl Future<Output = Result<Self::Transaction<'c>, Self::Error>> {
//! #         async { todo!() }
//! #     }
//! # }
//! #
//! # #[derive(Clone)]
//! # pub struct User(String);
//! #
//! # impl User {
//! #     pub fn new(name: &str) -> Self {
//! #         Self(name.to_string())
//! #     }
//! # }
//! #
//! # impl WriteToStore<Sqlite> for User {
//! #     async fn write(
//! #         &self,
//! #         tx: &mut <Sqlite as WritableStore>::Transaction<'_>,
//! #     ) -> Result<(), ()> {
//! #         Ok(())
//! #     }
//! # }
//! #
//! # pub struct Event {
//! #     title: String,
//! #     attendances: Vec<User>,
//! # }
//! #
//! # impl Event {
//! #     pub fn new(title: &str) -> Self {
//! #         Self {
//! #            title: title.to_string(),
//! #            attendances: vec![],
//! #         }
//! #     }
//! #
//! #     pub fn register_attendance(&mut self, user: &User) {
//! #         self.attendances.push(user.clone());
//! #     }
//! # }
//! #
//! # impl WriteToStore<Sqlite> for Event {
//! #     async fn write(
//! #         &self,
//! #         tx: &mut <Sqlite as WritableStore>::Transaction<'_>,
//! #     ) -> Result<(), ()> {
//! #         Ok(())
//! #     }
//! # }
//! #
//! # async fn run() -> Result<(), ()> {
//! // Initialise a concrete store implementation, for example for SQLite. This implementation
//! // needs to implement the `WritableStore` trait, providing it's native transaction interface.
//!
//! let mut store = Sqlite::new();
//!
//! // Establish state, do things with it. `User` and `Event` both implement `WriteToStore` for the
//! // concrete store type `Sqlite`.
//!
//! let user = User::new("casey");
//! let mut event = Event::new("Ants Research Meetup");
//! event.register_attendance(&user);
//!
//! // Persist state in database in one single, atomic transaction.
//!
//! let mut tx = store.begin().await?;
//!
//! user.write(&mut tx).await?;
//! event.write(&mut tx).await?;
//!
//! tx.commit().await?;
//!
//! # Ok(())
//! # }
//! ```
//!
//! It is recommended for application developers to re-use similar transaction patterns to leverage
//! the same crash-resiliance guarantees for their application-layer state and persistance
//! handling.
//!
//! ## Store implementations
//!
//! Read queries and atomic write transactions are implemented for all p2panda-stack related data
//! types for concrete databases: In-Memory and SQLite.
//!
//! An in-memory storage solution is provided in the form of a `MemoryStore` which implements both
//! `OperationStore` and `LogStore`. The store is gated by the `memory` feature flag and is enabled
//! by default.
//!
//! A SQLite storage solution is provided in the form of a `SqliteStore` which implements both
//! `OperationStore` and `LogStore`. The store is gated by the `sqlite` feature flag and is
//! disabled by default.
#[cfg(feature = "memory")]
pub mod memory;
pub mod operations;
#[cfg(feature = "sqlite")]
pub mod sqlite;
mod transactions;

#[cfg(feature = "memory")]
pub use memory::MemoryStore;
pub use operations::{
    BoxedLogStore, BoxedOperationStore, DynLogStore, DynOperationStore, LogId, LogStore,
    OperationStore, WrappedStore,
};
#[cfg(feature = "sqlite")]
pub use sqlite::store::{SqliteStore, SqliteStoreError};
pub use transactions::{Transaction, WritableStore, WriteToStore};
