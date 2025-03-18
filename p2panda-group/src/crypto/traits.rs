// SPDX-License-Identifier: MIT OR Apache-2.0

//! Traits for core cryptographic operations and random number generation.
use std::error::Error;

pub trait CryptoProvider {
    type Error: Error;

    type AeadNonce;

    type AeadKey;

    fn aead_encrypt(
        key: &Self::AeadKey,
        plaintext: &[u8],
        nonce: Self::AeadNonce,
        aad: Option<&[u8]>,
    ) -> Result<Vec<u8>, Self::Error>;

    fn aead_decrypt(
        key: &Self::AeadKey,
        ciphertext_tag: &[u8],
        nonce: Self::AeadNonce,
        aad: Option<&[u8]>,
    ) -> Result<Vec<u8>, Self::Error>;

    fn hkdf<const N: usize>(
        salt: &[u8],
        ikm: &[u8],
        info: Option<&[u8]>,
    ) -> Result<[u8; N], Self::Error>;
}

pub trait RandProvider {
    type Error: Error;

    fn random_array<const N: usize>(&self) -> Result<[u8; N], Self::Error>;

    fn random_vec(&self, len: usize) -> Result<Vec<u8>, Self::Error>;
}
