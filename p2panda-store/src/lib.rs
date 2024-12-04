// SPDX-License-Identifier: AGPL-3.0-or-later

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
#[cfg(feature = "memory")]
pub mod memory_store;

use std::fmt::{Debug, Display};

#[cfg(feature = "memory")]
pub use memory_store::MemoryStore;

use p2panda_core::{Body, Hash, Header, PublicKey, RawOperation};

/// Uniquely identify a single-author log.
///
/// The `LogId` exists purely to group a set of operations.
///
/// A blanket implementation is provided for any type meeting the required trait bounds.
pub trait LogId: Clone + Default + Debug + Eq + Send + Sync + std::hash::Hash {}

impl<T> LogId for T where T: Clone + Default + Debug + Eq + Send + Sync + std::hash::Hash {}

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
