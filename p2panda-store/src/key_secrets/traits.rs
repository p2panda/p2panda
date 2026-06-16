// SPDX-License-Identifier: MIT OR Apache-2.0

use std::error::Error;

/// Interface for setting and getting pre key secrets.
// TODO: Naming; should this rather be PreKeyBundlesStore?
pub trait KeySecretsStore<S> {
    type Error: Error;

    fn get_prekey_secrets(&self) -> impl Future<Output = Result<Option<S>, Self::Error>>;

    fn set_prekey_secrets(&self, state: &S) -> impl Future<Output = Result<(), Self::Error>>;
}
