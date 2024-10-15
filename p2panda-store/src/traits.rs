// SPDX-License-Identifier: AGPL-3.0-or-later

use p2panda_core::{Hash, Operation, PublicKey};
use thiserror::Error;

#[trait_variant::make(OperationStore: Send)]
pub trait LocalOperationStore<LogId, Extensions> {
    /// Insert an operation.
    ///
    /// Returns `true` when the insert occurred, or `false` when the operation already existed and
    /// no insertion occurred.
    async fn insert_operation(
        &mut self,
        operation: &Operation<Extensions>,
        log_id: &LogId,
    ) -> Result<bool, StoreError>;

    /// Get an operation.
    async fn get_operation(&self, hash: Hash) -> Result<Option<Operation<Extensions>>, StoreError>;

    /// Delete an operation.
    ///
    /// Returns `true` when the removal occurred and `false` when the operation was not found in
    /// the store.
    async fn delete_operation(&mut self, hash: Hash) -> Result<bool, StoreError>;

    /// Delete the payload of an operation.
    ///
    /// Returns `true` when the removal occurred and `false` when the operation was not found in
    /// the store or the payload was already deleted.
    async fn delete_payload(&mut self, hash: Hash) -> Result<bool, StoreError>;
}

#[trait_variant::make(RawStore: Send)]
pub trait LocalRawStore<Extensions> {
    /// Insert "raw" header bytes into store.
    ///
    /// Returns `true` when the insert occurred, or `false` when the header already existed and no
    /// insertion occurred.
    async fn insert_raw_header(&mut self, hash: Hash, bytes: &[u8]) -> Result<bool, StoreError>;

    /// Get "raw" header and body bytes of operation from store.
    async fn get_raw_operation(
        &self,
        hash: Hash,
    ) -> Result<Option<(Vec<u8>, Option<Vec<u8>>)>, StoreError>;

    /// Delete "raw" header bytes from store.
    ///
    /// Returns `true` when the removal occurred and `false` when the header was not found in
    /// the store.
    async fn delete_raw_header(&mut self, hash: Hash) -> Result<bool, StoreError>;
}

#[trait_variant::make(LogStore: Send)]
pub trait LocalLogStore<LogId, Extensions> {
    /// Get all operations from an authors' log ordered by sequence number.
    ///
    /// Returns an empty Vec when the author or a log with the requested id was not found.
    async fn get_log(
        &self,
        public_key: &PublicKey,
        log_id: &LogId,
    ) -> Result<Vec<Operation<Extensions>>, StoreError>;

    /// Get the log heights of all logs, by any author, which are stored under the passed log id.
    async fn get_log_heights(&self, log_id: &LogId) -> Result<Vec<(PublicKey, u64)>, StoreError>;

    /// Get only the latest operation from an authors' log.
    ///
    /// Returns None when the author or a log with the requested id was not found.
    async fn latest_operation(
        &self,
        public_key: &PublicKey,
        log_id: &LogId,
    ) -> Result<Option<Operation<Extensions>>, StoreError>;

    /// Delete all operations in a log before the given sequence number.
    ///
    /// Returns `true` when any operations were deleted, returns `false` when the author or log
    /// could not be found, or no operations were deleted.
    async fn delete_operations(
        &mut self,
        public_key: &PublicKey,
        log_id: &LogId,
        before: u64,
    ) -> Result<bool, StoreError>;

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
    ) -> Result<bool, StoreError>;
}

#[trait_variant::make(RawLogStore: Send)]
pub trait LocalRawLogStore<LogId, Extensions> {
    /// Get "raw" header and body bytes from an authors' log ordered by sequence number from a
    /// defined sequence number. If `from` is 0 the whole log is returned.
    ///
    /// Returns an empty list when the author or a log with the requested id was not found.
    async fn get_raw_log(
        &self,
        public_key: &PublicKey,
        log_id: &LogId,
        from: u64,
    ) -> Result<Vec<(Vec<u8>, Option<Vec<u8>>)>, StoreError>;

    /// Get the latest "raw" header and optional body bytes from an authors' log.
    ///
    /// Returns `None` when the author or a log with the requested id was not found.
    async fn latest_raw_operation(
        &self,
        public_key: &PublicKey,
        log_id: &LogId,
    ) -> Result<Option<(Vec<u8>, Option<Vec<u8>>)>, StoreError>;

    /// Delete all "raw" header bytes in a log before the given sequence number.
    ///
    /// Returns `true` when any raw headers were deleted, returns `false` when the author or log
    /// could not be found, or no headers were deleted.
    async fn delete_raw_headers(
        &mut self,
        public_key: &PublicKey,
        log_id: &LogId,
        before: u64,
    ) -> Result<bool, StoreError>;
}

#[derive(Error, Debug)]
pub enum StoreError {
    #[error("Error occurred in OperationStore: {0}")]
    OperationStoreError(String),
}
