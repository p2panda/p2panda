// SPDX-License-Identifier: MIT OR Apache-2.0

use std::error::Error;

/// Traits to implement database transaction provider.
///
/// To guard against sharing transactions unknowingly across unrelated database queries, a concept
/// of a "permit" was introduced which does not protect from misuse but helps to make "holding" a
/// transaction explicit.
pub trait Transaction {
    type Error: Error;

    type Permit;

    /// Begins a transaction.
    fn begin(&self) -> impl Future<Output = Result<Self::Permit, Self::Error>>;

    /// Rolls back the transaction and with that all uncommitted changes.
    fn rollback(&self, permit: Self::Permit) -> impl Future<Output = Result<(), Self::Error>>;

    /// Commits the transaction.
    fn commit(&self, permit: Self::Permit) -> impl Future<Output = Result<(), Self::Error>>;
}
