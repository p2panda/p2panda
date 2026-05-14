// SPDX-License-Identifier: MIT OR Apache-2.0

use std::error::Error;

use p2panda_core::Cursor;

/// Trait for storing, managing and querying cursors.
pub trait CursorStore<A, L> {
    type Error: Error;

    /// Returns the cursor matching the given name.
    ///
    /// Returns `None` if no cursor was found.
    fn get_cursor(
        &self,
        name: impl AsRef<str>,
    ) -> impl Future<Output = Result<Option<Cursor<A, L>>, Self::Error>>;

    /// Inserts the given cursor.
    ///
    /// Returns `true` if entry got inserted or `false` if existing entry was updated.
    fn set_cursor(&self, cursor: &Cursor<A, L>) -> impl Future<Output = Result<(), Self::Error>>;

    /// Deletes the cursor matching the given name.
    ///
    /// Returns `true` if entry was removed and `false` if it does not exist.
    fn delete_cursor(&self, name: impl AsRef<str>)
    -> impl Future<Output = Result<(), Self::Error>>;
}
