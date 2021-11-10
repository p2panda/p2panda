use memory_keystore::MemoryKeyStore;
use openmls_traits::crypto::OpenMlsCrypto;
use openmls_traits::random::OpenMlsRand;
use openmls_traits::types::{
    AeadType, CryptoError, HashType, HpkeCiphertext, HpkeConfig, HpkeKeyPair, SignatureScheme,
};
use openmls_traits::OpenMlsCryptoProvider;
use rust_crypto::RustCrypto;

use crate::identity::KeyPair;

/// Implements the `OpenMlsCryptoProvider` trait to be used as a crypto and key store backend for
/// all other MLS structs and methods.
///
/// This particular implementation allows us to pass in a custom key store and private key from the
/// outside for better configurability.
#[derive(Debug)]
pub struct MlsProvider {
    /// Provider for cryptographic methods.
    crypto: MlsCrypto,

    /// Provider to store and load KeyPackages.
    key_store: MemoryKeyStore,
}

impl MlsProvider {
    pub fn new(key_pair: KeyPair) -> Self {
        Self {
            crypto: MlsCrypto::new(key_pair),
            // @TODO: Use our own key store provider here
            key_store: MemoryKeyStore::default(),
        }
    }
}

impl OpenMlsCryptoProvider for MlsProvider {
    type CryptoProvider = MlsCrypto;
    type RandProvider = MlsCrypto;
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

/// Provider for random number generation, key-pair cryptography, signatures, hashing etc.
#[derive(Debug)]
pub struct MlsCrypto(RustCrypto, KeyPair);

impl MlsCrypto {
    fn new(key_pair: KeyPair) -> Self {
        Self(RustCrypto::default(), key_pair)
    }
}

impl OpenMlsCrypto for MlsCrypto {
    /// Check whether the [`SignatureScheme`] is supported or not. Returns an error if the
    /// signature scheme is not supported.
    fn supports(&self, signature_scheme: SignatureScheme) -> Result<(), CryptoError> {
        // We only support ed25519 here as this is the only needed scheme for p2panda.
        match signature_scheme {
            SignatureScheme::ED25519 => Ok(()),
            _ => Err(CryptoError::UnsupportedSignatureScheme),
        }
    }

    /// HKDF extract. Returns an error if the [`HashType`] is not supported.
    fn hkdf_extract(
        &self,
        hash_type: HashType,
        salt: &[u8],
        ikm: &[u8],
    ) -> Result<Vec<u8>, CryptoError> {
        self.0.hkdf_extract(hash_type, salt, ikm)
    }

    /// HKDF expand. Returns an error if the [`HashType`] is not supported or the output length is
    /// too long.
    fn hkdf_expand(
        &self,
        hash_type: HashType,
        prk: &[u8],
        info: &[u8],
        okm_len: usize,
    ) -> Result<Vec<u8>, CryptoError> {
        self.0.hkdf_expand(hash_type, prk, info, okm_len)
    }

    /// Hash the `data`. Returns an error if the [`HashType`] is not supported.
    fn hash(&self, hash_type: HashType, data: &[u8]) -> Result<Vec<u8>, CryptoError> {
        self.0.hash(hash_type, data)
    }

    /// AEAD encrypt with the given parameters. Returns an error if the [`AeadType`] is not
    /// supported or an encryption error occurs.
    fn aead_encrypt(
        &self,
        alg: AeadType,
        key: &[u8],
        data: &[u8],
        nonce: &[u8],
        aad: &[u8],
    ) -> Result<Vec<u8>, CryptoError> {
        self.0.aead_encrypt(alg, key, data, nonce, aad)
    }

    /// AEAD decrypt with the given parameters. Returns an error if the [`AeadType`] is not
    /// supported or a decryption error occurs.
    fn aead_decrypt(
        &self,
        alg: AeadType,
        key: &[u8],
        ct_tag: &[u8],
        nonce: &[u8],
        aad: &[u8],
    ) -> Result<Vec<u8>, CryptoError> {
        self.0.aead_decrypt(alg, key, ct_tag, nonce, aad)
    }

    /// Generate a signature key. Returns an error if the [`SignatureScheme`] is not supported or
    /// the key generation fails.
    fn signature_key_gen(&self, alg: SignatureScheme) -> Result<(Vec<u8>, Vec<u8>), CryptoError> {
        match alg {
            SignatureScheme::ED25519 => {
                let public_key = self.1.public_key().to_bytes();
                let private_key = self.1.private_key().to_bytes();
                // Full key here because we need it to sign
                let full_key = [private_key, public_key].concat();
                Ok((full_key.to_vec(), public_key.to_vec()))
            }
            _ => Err(CryptoError::UnsupportedSignatureScheme),
        }
    }

    /// Verify the signature. Returns an error if the [`SignatureScheme`] is not supported or the
    /// signature verification fails.
    fn verify_signature(
        &self,
        alg: SignatureScheme,
        data: &[u8],
        pk: &[u8],
        signature: &[u8],
    ) -> Result<(), CryptoError> {
        match alg {
            SignatureScheme::ED25519 => self.0.verify_signature(alg, data, pk, signature),
            _ => Err(CryptoError::UnsupportedSignatureScheme),
        }
    }

    /// Sign with the given parameters. Returns an error if the [`SignatureScheme`] is not
    /// supported or an error occurs during signature generation.
    fn sign(&self, alg: SignatureScheme, data: &[u8], key: &[u8]) -> Result<Vec<u8>, CryptoError> {
        match alg {
            SignatureScheme::ED25519 => self.0.sign(alg, data, key),
            _ => Err(CryptoError::UnsupportedSignatureScheme),
        }
    }

    /// HPKE single-shot encryption of `ptxt` to `pk_r`, using `info` and `aad`.
    fn hpke_seal(
        &self,
        config: HpkeConfig,
        pk_r: &[u8],
        info: &[u8],
        aad: &[u8],
        ptxt: &[u8],
    ) -> HpkeCiphertext {
        self.0.hpke_seal(config, pk_r, info, aad, ptxt)
    }

    /// HPKE single-shot decryption of `input` with `sk_r`, using `info` and `aad`.
    fn hpke_open(
        &self,
        config: HpkeConfig,
        input: &HpkeCiphertext,
        sk_r: &[u8],
        info: &[u8],
        aad: &[u8],
    ) -> Result<Vec<u8>, CryptoError> {
        self.0.hpke_open(config, input, sk_r, info, aad)
    }

    /// Derive a new HPKE keypair from a given input key material.
    fn derive_hpke_keypair(&self, config: HpkeConfig, ikm: &[u8]) -> HpkeKeyPair {
        self.0.derive_hpke_keypair(config, ikm)
    }
}

impl OpenMlsRand for MlsCrypto {
    type Error = &'static str;

    fn random_array<const N: usize>(&self) -> Result<[u8; N], Self::Error> {
        self.0.random_array()
    }

    fn random_vec(&self, len: usize) -> Result<Vec<u8>, Self::Error> {
        self.0.random_vec(len)
    }
}
