// SPDX-License-Identifier: MIT OR Apache-2.0

use std::error::Error;

use p2panda_core::VerifyingKey;
use p2panda_encryption::key_registry::KeyRegistryState;

/// Interface for setting and getting key registry state.
pub trait KeyRegistryStore {
    type Error: Error;

    fn get_key_registry(
        &self,
    ) -> impl Future<Output = Result<Option<KeyRegistryState<VerifyingKey>>, Self::Error>>;

    fn set_key_registry(
        &self,
        state: &KeyRegistryState<VerifyingKey>,
    ) -> impl Future<Output = Result<(), Self::Error>>;
}
