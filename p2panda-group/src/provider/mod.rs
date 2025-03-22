// SPDX-License-Identifier: MIT OR Apache-2.0

//! Trait interfaces and implementations for providing encryption, digital signing, hashing and
//! other cryptographic algorithms and random number generators.
#[cfg(feature = "provider")]
mod provider;
mod traits;

#[cfg(feature = "provider")]
pub use provider::{CryptoError, Provider, ProviderError, RandError};
pub use traits::{CryptoProvider, RandProvider, XCryptoProvider};
