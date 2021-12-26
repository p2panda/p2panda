// SPDX-License-Identifier: AGPL-3.0-or-later

use thiserror::Error;

/// Custom error types for Messaging Layer Security (MLS).
#[derive(Error, Debug)]
#[allow(missing_copy_implementations)]
pub enum MlsError {
    /// Internal MLS `KeyPackage` error.
    #[error(transparent)]
    KeyPackage(#[from] openmls::prelude::KeyPackageError),

    /// Internal MLS `ManagedGroup` error.
    #[error(transparent)]
    ManagedGroup(#[from] openmls::prelude::ManagedGroupError),

    /// Internal `memory_keystore` serialisation error.
    // @TODO: This will be changed as soon as we have our own key store implementation.
    #[error("KeyStore failed during serialisation")]
    KeyStoreSerialization,
}
