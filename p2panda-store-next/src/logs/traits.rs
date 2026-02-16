// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashMap;
use std::error::Error;

type LogEntries<T> = Vec<(T, Vec<u8>)>;

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
    ) -> impl Future<Output = Result<Option<LogEntries<T>>, Self::Error>>;

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
