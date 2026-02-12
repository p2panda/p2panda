// SPDX-License-Identifier: MIT OR Apache-2.0

use std::error::Error;

use p2panda_core::PublicKey;

type RawOperation = (Vec<u8>, Option<Vec<u8>>);

/// Interface for storing, deleting and querying operations.
///
/// The concrete type of an "operation" is generic and implementors can use the same interface for
/// different approaches: sets, append-only logs, hash-graphs (DAG) etc.
pub trait OperationStore<T, ID> {
    type Error: Error;

    /// Insert an operation.
    ///
    /// Returns `true` when the insert occurred, or `false` when the operation already existed and
    /// no insertion occurred.
    fn insert_operation(
        &self,
        id: &ID,
        operation: T,
    ) -> impl Future<Output = Result<bool, Self::Error>>;

    /// Get an operation by id.
    fn get_operation(&self, id: &ID) -> impl Future<Output = Result<Option<T>, Self::Error>>;

    /// Get a batch of operations.
    fn get_operations(&self, id: &[ID]) -> impl Future<Output = Result<Vec<T>, Self::Error>>;

    /// Get a batch of raw operation header and body bytes.
    fn get_raw_operations(
        &self,
        ids: &[ID],
    ) -> impl Future<Output = Result<Vec<RawOperation>, Self::Error>>;

    /// Get the byte count for a batch of operations.
    ///
    /// Needed for informing a remote how many bytes they should expect from us.
    fn byte_count(&self, ids: &[ID]) -> impl Future<Output = Result<u64, Self::Error>>;

    /// Query the existence of an operation.
    ///
    /// Returns `true` if the operation was found in the store and `false` if not.
    fn has_operation(&self, id: &ID) -> impl Future<Output = Result<bool, Self::Error>>;

    /// Delete an operation.
    ///
    /// Returns `true` when the removal occurred and `false` when the operation was not found in
    /// the store.
    fn delete_operation(&self, id: &ID) -> impl Future<Output = Result<bool, Self::Error>>;

    /// Delete an operation payload.
    ///
    /// Returns `true` when the removal occurred and `false` when the operation was not found in
    /// the store.
    fn delete_operation_payload(&self, id: &ID) -> impl Future<Output = Result<bool, Self::Error>>;
}

type SeqNum = u64;

/// Store methods for aiding efficient comparison of log-based data types.
/// 
/// The concrete message type contained on each log "entry" is not known in this API. It is
/// assumed there is another store for retrieving these during sync (eg. OperationStore).
pub trait LogStore<L, ID> {
    type Error: Error;

    /// Efficiently get all log heights for a set of author logs.
    fn get_frontiers(
        &self,
        author: &PublicKey,
        logs: Vec<L>,
    ) -> impl Future<Output = Result<Option<Vec<(L, Option<(ID, SeqNum)>)>>, Self::Error>>;

    /// Get all entries in a log from an optional starting seq num.
    fn log_entries(
        &self,
        author: &PublicKey,
        log_id: &L,
        from: Option<SeqNum>,
    ) -> impl Future<Output = Result<Option<Vec<(ID, SeqNum)>>, Self::Error>>;

    /// Efficiently prune a log.
    fn delete_entries(
        &mut self,
        author: &PublicKey,
        log_id: &L,
        before: u64,
    ) -> impl Future<Output = Result<bool, Self::Error>>;
}
