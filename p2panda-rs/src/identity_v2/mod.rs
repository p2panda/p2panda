// SPDX-License-Identifier: AGPL-3.0-or-later

//! Generates and maintains Ed25519 key pairs for digital signatures.
pub mod error;
mod key_pair;
mod private_key;
mod public_key;
mod signature;

pub use key_pair::KeyPair;
pub use private_key::PrivateKey;
pub use public_key::PublicKey;
pub use signature::Signature;
