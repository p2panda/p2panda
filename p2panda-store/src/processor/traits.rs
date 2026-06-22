// SPDX-License-Identifier: MIT OR Apache-2.0

use std::error::Error;

use p2panda_core::Hash;

pub trait ProcessorStore<T> {
    type Error: Error;

    // TODO: Are we happy for `id` to be a concrete type here?
    fn get_event(&self, id: &Hash) -> impl Future<Output = Result<Option<T>, Self::Error>>;

    fn set_event(&self, id: &Hash, event: &T) -> impl Future<Output = Result<(), Self::Error>>;
}
