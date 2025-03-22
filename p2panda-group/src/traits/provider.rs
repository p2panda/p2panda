// SPDX-License-Identifier: MIT OR Apache-2.0

//! Interface definitions for providing cryptographic algorithms and random number generators used
//! by p2panda's group encryption schemes.
use std::error::Error;

/// Provider for Authenticated Encryption with Additional Data (AEAD), Hybrid Public Key Encryption
/// (HPKE), Hybrid Key Derivation Function (HKDF), Digital Signature Algorithm (DSA) and
/// Cryptographically Secure Hashing.
pub trait CryptoProvider {
    type Error: Error;

    type AeadNonce;

    type AeadKey;

    type PublicKey;

    type SecretKey;

    type HpkeCiphertext;

    type SigningKey;

    type VerifyingKey;

    type Signature;

    fn aead_encrypt(
        &self,
        key: &Self::AeadKey,
        plaintext: &[u8],
        nonce: Self::AeadNonce,
        aad: Option<&[u8]>,
    ) -> Result<Vec<u8>, Self::Error>;

    fn aead_decrypt(
        &self,
        key: &Self::AeadKey,
        ciphertext_tag: &[u8],
        nonce: Self::AeadNonce,
        aad: Option<&[u8]>,
    ) -> Result<Vec<u8>, Self::Error>;

    fn hkdf<const N: usize>(
        &self,
        salt: &[u8],
        ikm: &[u8],
        info: Option<&[u8]>,
    ) -> Result<[u8; N], Self::Error>;

    fn hpke_seal(
        &self,
        public_key: &Self::PublicKey,
        info: Option<&[u8]>,
        aad: Option<&[u8]>,
        plaintext: &[u8],
    ) -> Result<Self::HpkeCiphertext, <Self as CryptoProvider>::Error>;

    fn hpke_open(
        &self,
        input: &Self::HpkeCiphertext,
        secret_key: &Self::SecretKey,
        info: Option<&[u8]>,
        aad: Option<&[u8]>,
    ) -> Result<Vec<u8>, Self::Error>;

    fn hash(&self, bytes: &[&[u8]]) -> Result<Vec<u8>, Self::Error>;

    fn sign(
        &self,
        bytes: &[u8],
        signing_key: &Self::SigningKey,
    ) -> Result<Self::Signature, Self::Error>;

    fn verify(
        &self,
        bytes: &[u8],
        verifying_key: &Self::VerifyingKey,
        signature: &Self::Signature,
    ) -> Result<(), Self::Error>;
}

/// Provider for "extended" AEAD and hybrid DSA / Key Agreement algorithms.
///
/// Depending on the group encryption scheme we sometimes need specialised cryptographic algorithms
/// which are not standardised. This provider offers implementation interfaces to AEAD schemes with
/// larger nonces (allowing nonces to be generated randomly while preventing collisions leading to
/// nonce re-use) and hybrid "EdDSA" (public-key encryption algorithms used for both DSA and key
/// agreement).
pub trait XCryptoProvider {
    type Error: Error;

    type XAeadNonce;

    type XAeadKey;

    type XSigningKey;

    type XVerifyingKey;

    type XSignature;

    fn x_aead_encrypt(
        &self,
        key: &Self::XAeadKey,
        plaintext: &[u8],
        nonce: Self::XAeadNonce,
        aad: Option<&[u8]>,
    ) -> Result<Vec<u8>, Self::Error>;

    fn x_aead_decrypt(
        &self,
        key: &Self::XAeadKey,
        ciphertext_tag: &[u8],
        nonce: Self::XAeadNonce,
        aad: Option<&[u8]>,
    ) -> Result<Vec<u8>, Self::Error>;

    fn x_sign(
        &self,
        bytes: &[u8],
        signing_key: &Self::XSigningKey,
    ) -> Result<Self::XSignature, Self::Error>;

    fn x_verify(
        &self,
        bytes: &[u8],
        verifying_key: &Self::XVerifyingKey,
        signature: &Self::XSignature,
    ) -> Result<(), Self::Error>;

    fn x_calculate_agreement(
        &self,
        secret_key: &Self::XSigningKey,
        public_key: &Self::XVerifyingKey,
    ) -> Result<Vec<u8>, Self::Error>;
}

/// Provider for a Cryptographically Secure Pseudo-Random Number Generator (CSPRNG).
pub trait RandProvider {
    type Error: Error;

    fn random_array<const N: usize>(&self) -> Result<[u8; N], Self::Error>;

    fn random_vec(&self, len: usize) -> Result<Vec<u8>, Self::Error>;
}
