// SPDX-License-Identifier: AGPL-3.0-or-later

use memory_keystore::MemoryKeyStore;
use openmls_traits::OpenMlsCryptoProvider;
use rust_crypto::RustCrypto;

/// Implements the `OpenMlsCryptoProvider` trait to be used as a crypto and key store backend for
/// all other MLS structs and methods.
///
/// @TODO: This will use our own key store soon.
#[derive(Debug)]
pub struct MlsProvider {
    /// Provider for cryptographic methods.
    crypto: RustCrypto,

    /// Provider to store and load KeyPackages.
    key_store: MemoryKeyStore,
}

impl MlsProvider {
    /// Creates a new instance of the `MlsProvider`.
    pub fn new() -> Self {
        Self {
            crypto: RustCrypto::default(),
            key_store: MemoryKeyStore::default(),
        }
    }
}

impl OpenMlsCryptoProvider for MlsProvider {
    type CryptoProvider = RustCrypto;
    type RandProvider = RustCrypto;
    type KeyStoreProvider = MemoryKeyStore;

    fn crypto(&self) -> &Self::CryptoProvider {
        &self.crypto
    }

    fn rand(&self) -> &Self::RandProvider {
        &self.crypto
    }

    fn key_store(&self) -> &Self::KeyStoreProvider {
        &self.key_store
    }
}
