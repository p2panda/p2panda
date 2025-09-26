// SPDX-License-Identifier: MIT OR Apache-2.0

use std::error::Error;

/// Interface for storing, deleting and querying operations.
///
/// The concrete type of an "operation" is generic and implementors can use the same interface for
/// different approaches: sets, append-only logs or hash-graphs etc.
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
    fn get_operation(&self, id: &ID) -> impl Future<Output = Result<T, Self::Error>>;

    /// Query the existence of an operation.
    ///
    /// Returns `true` if the operation was found in the store and `false` if not.
    fn has_operation(&self, id: &ID) -> impl Future<Output = Result<bool, Self::Error>>;

    /// Delete an operation.
    ///
    /// Returns `true` when the removal occurred and `false` when the operation was not found in
    /// the store.
    fn delete_operation(&self, id: &ID) -> impl Future<Output = Result<bool, Self::Error>>;
}
