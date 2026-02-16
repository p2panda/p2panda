// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashMap;
use std::error::Error;
use std::fmt::Debug;

use p2panda_core::PublicKey;
use p2panda_core::logs::StateVector;

use crate::operations::SeqNum;

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

/// Store methods for aiding efficient comparison of log-based data types.
///
/// The concrete message type contained on each log "entry" is not known in this API. It is
/// assumed there is another store for retrieving these during sync (eg. OperationStore).
pub trait LogStore<T, L, ID> {
    type Error: Error;

    /// Query the existence of a log entry.
    ///
    /// Returns `true` if the entry was found in the store and `false` if not.
    fn has_entry(
        &self,
        log_id: &L,
        id: &ID,
        seq_num: SeqNum,
    ) -> impl Future<Output = Result<bool, Self::Error>>;

    /// Efficiently get current frontiers for a set of author logs.
    ///
    /// The frontiers of a set of logs are needed when communicating our current state to a remote
    /// peer during set. Including the "frontiers" (set of hash+seq_num tuples) allows for
    /// handling of forked logs.
    ///
    /// @NOTE: should we maintain a "frontiers" table where we maintain current frontiers for all
    /// logs when we write new operations?
    fn get_frontiers(
        &self,
        author: &PublicKey,
        logs: &[L],
    ) -> impl Future<Output = Result<Option<HashMap<L, StateVector>>, Self::Error>>;

    /// Get the current height of a log.
    ///
    /// Used mostly when new log entries are being forged and the current height of a log needs to
    /// be known in order to set a "backlink" and sequence number. In the case of a log being
    /// forked it is expected that the entry with the heighest seq_num will be chosen, with the
    /// hash being used as a tiebreaker when needed.
    ///
    /// Returns None when the author or a log with the requested id was not found.
    fn get_log_height(
        &self,
        public_key: &PublicKey,
        log_id: &L,
    ) -> impl Future<Output = Result<Option<(ID, SeqNum)>, Self::Error>>;

    /// Get the byte and operation count of the entries in a log.
    ///
    /// The entry to start after can optionally be set by it's seq num.
    fn get_log_size(
        &self,
        public_key: &PublicKey,
        log_id: &L,
        after: Option<SeqNum>,
    ) -> impl Future<Output = Result<Option<(u64, u64)>, Self::Error>>;

    /// Get all entries in a log after an optional starting point.
    fn get_log_entries(
        &self,
        public_key: &PublicKey,
        log_id: &L,
        // @TODO: we could use the Hash id of a log entry here instead. This would stop us
        // "accidentally" making queries based on the seq number of a log branch that we don't
        // actually have locally (it would save us one db query).
        after: Option<SeqNum>,
    ) -> impl Future<Output = Result<Option<Vec<T>>, Self::Error>>;

    /// Efficiently prune a log.
    fn delete_entries(
        &self,
        author: &PublicKey,
        log_id: &L,
        before: &SeqNum,
    ) -> impl Future<Output = Result<bool, Self::Error>>;

    // == OPTIONAL BATCH QUERIES == //

    // /// Get all entries in a set of logs after provided height.
    // fn get_logs_entries_batch(
    //     &self,
    //     public_key: &PublicKey,
    //     log_heights: &HashMap<L, Height<ID>>,
    // ) -> impl Future<Output = Result<Option<(u64, u64)>, Self::Error>>;

    // /// Get the byte and operation count of the entries in a set of logs.
    // fn get_log_size_batch(
    //     &self,
    //     public_key: &PublicKey,
    //     log_heights: &HashMap<L, Height<ID>>,
    // ) -> impl Future<Output = Result<Option<(u64, u64)>, Self::Error>>;
}
