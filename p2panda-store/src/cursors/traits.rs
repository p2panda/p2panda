// SPDX-License-Identifier: MIT OR Apache-2.0

use std::error::Error;

use p2panda_core::Cursor;

pub trait CursorStore<A, L> {
    type Error: Error;

    fn get_cursor(
        &self,
        name: impl AsRef<str>,
    ) -> impl Future<Output = Result<Option<Cursor<A, L>>, Self::Error>>;

    fn set_cursor(&self, cursor: &Cursor<A, L>) -> impl Future<Output = Result<(), Self::Error>>;

    fn delete_cursor(&self, name: impl AsRef<str>)
    -> impl Future<Output = Result<(), Self::Error>>;
}
