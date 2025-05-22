// SPDX-License-Identifier: MIT OR Apache-2.0

//! Core cryptographic algorithms and random number generator.
//!
//! "Basic" Algorithms:
//! - DHKEM-X25519 HPKE
//! - SHA256 HKDF
//! - ChaCha20Poly1305 AEAD
//! - Ed25519 (SHA512) DSA
//!
//! "Extended" Algorithms:
//! - XEdDSA (DSA with X25519)
//! - XChaCha20Poly1305 (large IVs)
//!
//! Random Number Generator:
//! - ChaCha20 stream cipher, seeded via `getrandom`
pub(crate) mod aead;
pub(crate) mod hkdf;
pub(crate) mod hpke;
mod rng;
mod secret;
pub(crate) mod sha2;
pub(crate) mod x25519;
pub(crate) mod xchacha20;
pub(crate) mod xeddsa;

pub use aead::{AeadError, AeadKey, AeadNonce, aead_decrypt, aead_encrypt};
pub use rng::{Rng, RngError};
pub use secret::Secret;
pub use x25519::{PUBLIC_KEY_SIZE, PublicKey, SECRET_KEY_SIZE, SHARED_SECRET_SIZE, SecretKey};
pub use xchacha20::{XAeadError, XAeadKey, XAeadNonce, x_aead_decrypt, x_aead_encrypt};
