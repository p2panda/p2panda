// SPDX-License-Identifier: AGPL-3.0-or-later

use openmls_memory_keystore::MemoryKeyStore;
use openmls_rust_crypto::RustCrypto;
use openmls_traits::OpenMlsCryptoProvider;

/// Implements the `OpenMlsCryptoProvider` trait to be used as a crypto and key store backend for
/// all other MLS structs and methods.
///
/// OpenMLS does not implement its own cryptographic primitives and storage solution for key
/// material. Instead, it relies on existing implementations of the cryptographic primitives and
/// storage backends. We introduce our approach to OpenMLS in form of this `OpenMlsCryptoProvider`
/// trait implementation.
///
/// @TODO: This will use our own key store soon.
#[derive(Debug)]
pub struct MlsProvider {
    /// Backend for cryptographic primitives.
    crypto: RustCrypto,

    /// Backend to store and load KeyPackages and Credentials.
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

impl Default for MlsProvider {
    fn default() -> Self {
        Self::new()
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
