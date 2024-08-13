// SPDX-License-Identifier: AGPL-3.0-or-later

use p2panda_core::{Hash, Operation, PublicKey};
use thiserror::Error;

pub trait OperationStore<LogId, Extensions> {
    /// Insert an operation.
    ///
    /// Returns `true` when the insert occurred, or `false` when the operation
    /// already existed and no insertion occurred.
    fn insert_operation(&mut self, operation: Operation<Extensions>, log_id: LogId) -> Result<bool, StoreError>;

    /// Get an operation.
    fn get_operation(&self, hash: Hash) -> Result<Option<Operation<Extensions>>, StoreError>;

    /// Delete an operation.
    ///
    /// Returns `true` when the removal occurred and `false` when the operation
    /// was not found in the store.
    fn delete_operation(&mut self, hash: Hash) -> Result<bool, StoreError>;

    /// Delete the payload of an operation.
    ///
    /// Returns `true` when the removal occurred and `false` when the operation
    /// was not found in the store or the payload was already deleted.
    fn delete_payload(&mut self, hash: Hash) -> Result<bool, StoreError>;
}

pub trait LogStore<LogId, Extensions> {
    /// Get all operations from an authors' log ordered by sequence number.
    ///
    /// Returns None when the author or a log with the requested id was not found.
    fn get_log(
        &self,
        public_key: PublicKey,
        log_id: LogId,
    ) -> Result<Option<Vec<Operation<Extensions>>>, StoreError>;

    /// Get only the latest operation from an authors' log.
    ///
    /// Returns None when the author or a log with the requested id was not found.
    fn latest_operation(
        &self,
        public_key: PublicKey,
        log_id: LogId,
    ) -> Result<Option<Operation<Extensions>>, StoreError>;

    /// Delete a range of operations from an authors' log.
    ///
    /// Returns `true` when operations within the requested range were deleted, or `false` when
    /// the author or log could not be found, or no operations were deleted.
    fn delete_operations(
        &mut self,
        public_key: PublicKey,
        log_id: LogId,
        from: u64,
        to: Option<u64>,
    ) -> Result<bool, StoreError>;

    /// Delete a range of operation payloads from an authors' log.
    ///
    /// Returns `true` when operations within the requested range were deleted, or `false` when
    /// the author or log could not be found, or no operations were deleted.
    fn delete_payloads(
        &mut self,
        public_key: PublicKey,
        log_id: LogId,
        from: u64,
        to: Option<u64>,
    ) -> Result<bool, StoreError>;
}

pub trait StreamStore<StreamId, Extensions> {
    /// Get all operations from a stream.
    ///
    /// A stream contains operations from all author logs which share the same `LogId`.
    /// Conceptually they can be understood as multi-writer logs. The operations in the returned
    /// collection are "locally" ordered (ordered by sequence number per-log) but globally
    /// unordered.
    fn get_stream(stream_name: StreamId) -> Result<Option<Vec<Operation<Extensions>>>, StoreError>;
}

#[derive(Error, Debug)]
pub enum StoreError {
    #[error("Error occurred in OperationStore: {0}")]
    OperationStoreError(String),
}
