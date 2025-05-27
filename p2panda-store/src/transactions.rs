// SPDX-License-Identifier: MIT OR Apache-2.0

//! Trait definitions for atomic write transactions.

/// Store implementation returning an atomic transaction object for fail-safe writes.
pub trait WritableStore {
    /// Error type from store.
    type Error;

    /// Store type for fail-safe transactions.
    type Transaction<'c>: Transaction;

    /// Returns new transaction object to "begin" a single, atomic write transaction which is
    /// finally "committed" into the store.
    fn begin<'c>(&mut self) -> impl Future<Output = Result<Self::Transaction<'c>, Self::Error>>;
}

/// Writes state changes into a store as part of an atomic transaction.
///
/// Developers should implement this trait on types which represent state which needs persisting or
/// the "delta" which needs changing. On `write` the concrete query (for example a SQL insert or
/// update) is executed as part of an atomic transaction.
///
/// The "written" object can usually be dropped after a successful transaction.
pub trait WriteToStore<S: WritableStore> {
    fn write(&self, tx: &mut S::Transaction<'_>) -> impl Future<Output = Result<(), S::Error>>;
}

/// Organises multiple "writes" to a store into one atomic transaction.
pub trait Transaction {
    /// Error type from store which can occur when writing to it or rolling back.
    type Error;

    /// Finally "commits" all writes to the store as one single "transaction".
    fn commit(self) -> impl Future<Output = Result<(), Self::Error>>;

    /// Aborts writing to the store and "rolls back" all changes. This should automatically be
    /// called on `Drop`.
    fn rollback(self) -> impl Future<Output = Result<(), Self::Error>>;
}
