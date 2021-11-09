use memory_keystore::MemoryKeyStore;
use openmls_traits::OpenMlsCryptoProvider;
use rust_crypto::RustCrypto;

/// Implements the `OpenMlsCryptoProvider` trait to be used as a crypto and key store backend for
/// all other MLS structs and methods.
///
/// This particular implementation allows us to pass in a custom key store and private key from the
/// outside for better configurability.
#[derive(Default, Debug)]
pub struct MlsProvider {
    crypto: RustCrypto,
    key_store: MemoryKeyStore,
}

impl MlsProvider {
    pub fn new() -> Self {
        Self {
            // @TODO: Give option to pass in private key into crypto provider
            crypto: RustCrypto::default(),
            // @TODO: Use our own key store provider here
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
