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
pub mod aead;
pub mod hkdf;
pub mod hpke;
mod rng;
mod secret;
pub mod sha2;
pub mod x25519;
pub mod xchacha20;
pub mod xeddsa;

pub use rng::{Rng, RngError};
pub use secret::Secret;
