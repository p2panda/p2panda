// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fmt::Debug;

use p2panda_encryption::key_registry::KeyRegistryState;
use p2panda_spaces::ActorId;

/// Interface for setting and getting key registry state.
pub trait KeyRegistryStore {
    type Error: Debug;

    fn get_key_registry(
        &self,
    ) -> impl Future<Output = Result<Option<KeyRegistryState<ActorId>>, Self::Error>>;

    fn set_key_registry(
        &self,
        y: &KeyRegistryState<ActorId>,
    ) -> impl Future<Output = Result<(), Self::Error>>;
}
