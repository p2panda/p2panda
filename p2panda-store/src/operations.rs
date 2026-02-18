// SPDX-License-Identifier: MIT OR Apache-2.0

//! Trait definitions for read-only queries on p2panda operation and log state.
use std::fmt::{Debug, Display};

use p2panda_core::{Body, Hash, Header, PublicKey, RawOperation};

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

    /// Get the sum of header and body bytes from an authors' log.
    ///
    /// The `from` value will be used as the starting index for log retrieval, if supplied,
    /// otherwise the sum of all operation bytes will be returned.
    ///
    /// Returns `None` when either the author or a log with the requested id was not found.
    async fn get_log_size(
        &self,
        public_key: &PublicKey,
        log_id: &LogId,
        from: Option<u64>,
    ) -> Result<Option<u64>, Self::Error>;

    /// Get hashes from an authors' log ordered by sequence number.
    ///
    /// The `from` value will be used as the starting index for log retrieval, if supplied,
    /// otherwise hashes for all operations will be returned.
    ///
    /// Returns `None` when either the author or a log with the requested id was not found.
    async fn get_log_hashes(
        &self,
        public_key: &PublicKey,
        log_id: &LogId,
        from: Option<u64>,
    ) -> Result<Option<Vec<(u64, Hash)>>, Self::Error>;

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
