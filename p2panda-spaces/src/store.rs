// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fmt::Debug;

use crate::encryption::key_manager::KeyManagerState;
use crate::encryption::key_registry::KeyRegistryState;

pub trait SpacesStore {
    type Error: Debug;

    fn key_manager(&self) -> impl Future<Output = Result<KeyManagerState, Self::Error>>;

    fn key_registry(&self) -> impl Future<Output = Result<KeyRegistryState, Self::Error>>;
}
