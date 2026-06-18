// SPDX-License-Identifier: MIT OR Apache-2.0

use std::error::Error;

use p2panda_encryption::key_manager::PreKeyBundlesState;

/// Interface for setting and getting pre key secrets.
// TODO: Naming; should this rather be PreKeyBundlesStore?
pub trait KeySecretsStore {
    type Error: Error;

    fn get_prekey_secrets(
        &self,
    ) -> impl Future<Output = Result<Option<PreKeyBundlesState>, Self::Error>>;

    fn set_prekey_secrets(
        &self,
        state: &PreKeyBundlesState,
    ) -> impl Future<Output = Result<(), Self::Error>>;
}
