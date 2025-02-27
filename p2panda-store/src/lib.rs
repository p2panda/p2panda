// SPDX-License-Identifier: MIT OR Apache-2.0

#![cfg_attr(doctest, doc=include_str!("../README.md"))]

//! Interfaces and implementations of persistence layers for core p2panda data types.
//!
//! The provided APIs allow for efficient implementations of `Operation` and log stores. These
//! persistence and query APIs are utilised by higher-level components of the p2panda stack, such
//! as `p2panda-sync` and `p2panda-stream`. For detailed information concerning the `Operation`
//! type, please consult the documentation for the `p2panda-core` crate.
//!
//! Logs in the context of `p2panda-store` are simply a collection of operations grouped under a
//! common identifier. The precise type for which `LogId` is implemented is left up to the
//! developer to decide according to their needs. With this in mind, the traits and implementations
//! provided by `p2panda-store` do not perform any validation of log integrity. Developers using
//! this crate must take steps to ensure their log design is fit for purpose and that all operations
//! have been thoroughly validated before being persisted.
//!
//! Also note that the traits provided here are not intended to offer generic storage solutions for
//! non-p2panda data types, nor are they intended to solve application-layer storage concerns.
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
#[cfg(feature = "sqlite")]
pub mod sqlite;

#[cfg(feature = "memory")]
pub use memory::MemoryStore;
#[cfg(feature = "sqlite")]
pub use sqlite::store::{SqliteStore, SqliteStoreError};

use std::fmt::{Debug, Display};

use p2panda_core::{Body, Hash, Header, PublicKey, RawOperation};

/// Uniquely identify a single-author log.
///
/// The `LogId` exists purely to group a set of operations and is intended to be implemented for
/// any type which meets the design requirements of a particular application.
///
/// A blanket implementation is provided for any type meeting the required trait bounds.
///
/// Here we briefly outline several implementation scenarios:
///
/// An application relying on a one-log-per-author design might choose to implement `LogId` for a thin
/// wrapper around an Ed25519 public key; this effectively ties the log to the public key of the
/// author. Secure Scuttlebutt (SSB) is an example of a protocol which relies on this model.
///
/// In an application where one author may produce operations grouped into multiple logs,
/// `LogId` might be implemented for a `struct` which includes both the public key of the author
/// and a unique number for each log instance.
///
/// Some applications might require semantic grouping of operations. For example, a chat
/// application may choose to create a separate log for each author-channel pairing. In such a
/// scenario, `LogId` might be implemented for a `struct` containing a `String` representation of
/// the channel name.
///
/// Finally, please note that implementers of `LogId` must take steps to ensure their log design is
/// fit for purpose and that all operations have been thoroughly validated before being persisted.
/// No such validation checks are provided by `p2panda-store`.
pub trait LogId: Clone + Debug + Eq + std::hash::Hash {}

impl<T> LogId for T where T: Clone + Debug + Eq + std::hash::Hash {}

/// Interface for storing, deleting and querying operations.
///
/// Two variants of the trait are provided: one which is thread-safe (implementing `Sync`) and one
/// which is purely intended for single-threaded execution contexts.
#[trait_variant::make(OperationStore: Send)]
pub trait LocalOperationStore<LogId, Extensions>: Clone {
    type Error: Display + Debug;

    /// Insert an operation.
    ///
    /// Returns `true` when the insert occurred, or `false` when the operation already existed and
    /// no insertion occurred.
    async fn insert_operation(
        &mut self,
        hash: Hash,
        header: &Header<Extensions>,
        body: Option<&Body>,
        header_bytes: &[u8],
        log_id: &LogId,
    ) -> Result<bool, Self::Error>;

    /// Get an operation.
    async fn get_operation(
        &self,
        hash: Hash,
    ) -> Result<Option<(Header<Extensions>, Option<Body>)>, Self::Error>;

    /// Get the "raw" header and body bytes of an operation.
    async fn get_raw_operation(&self, hash: Hash) -> Result<Option<RawOperation>, Self::Error>;

    /// Query the existence of an operation.
    ///
    /// Returns `true` if the operation was found in the store and `false` if not.
    async fn has_operation(&self, hash: Hash) -> Result<bool, Self::Error>;

    /// Delete an operation.
    ///
    /// Returns `true` when the removal occurred and `false` when the operation was not found in
    /// the store.
    async fn delete_operation(&mut self, hash: Hash) -> Result<bool, Self::Error>;

    /// Delete the payload of an operation.
    ///
    /// Returns `true` when the removal occurred and `false` when the operation was not found in
    /// the store or the payload was already deleted.
    async fn delete_payload(&mut self, hash: Hash) -> Result<bool, Self::Error>;
}

/// Interface for storing, deleting and querying logs.
///
/// Two variants of the trait are provided: one which is thread-safe (implementing `Sync`) and one
/// which is purely intended for single-threaded execution contexts.
#[trait_variant::make(LogStore: Send)]
pub trait LocalLogStore<LogId, Extensions> {
    type Error: Display + Debug;

    /// Get operations from an authors' log ordered by sequence number.
    ///
    /// The `from` value will be used as the starting index for log retrieval, if supplied,
    /// otherwise all operations will be returned.
    ///
    /// Returns `None` when either the author or a log with the requested id was not found.
    async fn get_log(
        &self,
        public_key: &PublicKey,
        log_id: &LogId,
        from: Option<u64>,
    ) -> Result<Option<Vec<(Header<Extensions>, Option<Body>)>>, Self::Error>;

    /// Get "raw" header and body bytes from an authors' log ordered by sequence number.
    ///
    /// The `from` value will be used as the starting index for log retrieval, if supplied,
    /// otherwise all operations will be returned.
    ///
    /// Returns `None` when either the author or a log with the requested id was not found.
    async fn get_raw_log(
        &self,
        public_key: &PublicKey,
        log_id: &LogId,
        from: Option<u64>,
    ) -> Result<Option<Vec<RawOperation>>, Self::Error>;

    /// Get the log heights of all logs, by any author, which are stored under the passed log id.
    async fn get_log_heights(&self, log_id: &LogId) -> Result<Vec<(PublicKey, u64)>, Self::Error>;

    /// Get only the latest operation from an authors' log.
    ///
    /// Returns None when the author or a log with the requested id was not found.
    async fn latest_operation(
        &self,
        public_key: &PublicKey,
        log_id: &LogId,
    ) -> Result<Option<(Header<Extensions>, Option<Body>)>, Self::Error>;

    /// Delete all operations in a log before the given sequence number.
    ///
    /// Returns `true` when any operations were deleted, returns `false` when the author or log
    /// could not be found, or no operations were deleted.
    async fn delete_operations(
        &mut self,
        public_key: &PublicKey,
        log_id: &LogId,
        before: u64,
    ) -> Result<bool, Self::Error>;

    /// Delete a range of operation payloads in an authors' log.
    ///
    /// The range of deleted payloads includes it's lower bound `from` but excludes the upper bound
    /// `to`.
    ///
    /// Returns `true` when operations within the requested range were deleted, or `false` when the
    /// author or log could not be found, or no operations were deleted.
    async fn delete_payloads(
        &mut self,
        public_key: &PublicKey,
        log_id: &LogId,
        from: u64,
        to: u64,
    ) -> Result<bool, Self::Error>;
}
