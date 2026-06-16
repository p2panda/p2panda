// SPDX-License-Identifier: MIT OR Apache-2.0

use std::error::Error;

/// Interface for setting and getting key registry state.
pub trait KeyRegistryStore<S> {
    type Error: Error;

    fn get_key_registry(&self) -> impl Future<Output = Result<Option<S>, Self::Error>>;

    fn set_key_registry(&self, state: &S) -> impl Future<Output = Result<(), Self::Error>>;
}
