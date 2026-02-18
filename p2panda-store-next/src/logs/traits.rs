// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashMap;
use std::error::Error;
use std::fmt::Debug;

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
pub trait LogStore<T, A, L, S, ID> {
    type Error: Error;

    /// Get the ID and sequence number of the latest entry in a log.
    ///
    /// Returns None when the author or a log with the requested id was not found.
    fn get_latest_entry(
        &self,
        author: &A,
        log_id: &L,
    ) -> impl Future<Output = Result<Option<(ID, S)>, Self::Error>>;

    /// Get current heights for a set of logs.
    ///
    /// Returns the sequence number for the latest entry in every requested log.
    ///
    /// Returns None when the author or a log with the requested id was not found.
    fn get_log_heights(
        &self,
        author: &A,
        logs: &[L],
    ) -> impl Future<Output = Result<Option<HashMap<L, S>>, Self::Error>>;

    /// Get the byte and operation count of the entries in a log.
    ///
    /// `after` and `until` fields can be provided to select only a range of the log.
    fn get_log_size(
        &self,
        author: &A,
        log_id: &L,
        after: Option<S>,
        until: Option<S>,
    ) -> impl Future<Output = Result<Option<(u64, u64)>, Self::Error>>;

    /// Get all entries in a log after an optional starting point.
    ///
    /// `after` and `to` fields can be provided to select only a range of the log.
    fn get_log_entries(
        &self,
        author: &A,
        log_id: &L,
        after: Option<S>,
        until: Option<S>,
    ) -> impl Future<Output = Result<Option<Vec<(T, Vec<u8>)>>, Self::Error>>;

    /// Prune all entries in a log until the provided sequence number.
    ///
    /// Returns the number of entries which were pruned.
    fn prune_entries(
        &self,
        author: &A,
        log_id: &L,
        until: &S,
    ) -> impl Future<Output = Result<u64, Self::Error>>;
}
