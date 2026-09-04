// SPDX-License-Identifier: MIT OR Apache-2.0

use std::error::Error;

use p2panda_core::Hash;

// TODO: Is `ProcessorStore` the best name? Maybe `EventStore` or `EventMetadataStore` is more
// specific and thus clearer?
pub trait ProcessorStore<T> {
    type Error: Error;

    fn get_event(&self, id: &Hash) -> impl Future<Output = Result<Option<T>, Self::Error>>;

    fn set_event(&self, id: &Hash, event: &T) -> impl Future<Output = Result<(), Self::Error>>;
}
