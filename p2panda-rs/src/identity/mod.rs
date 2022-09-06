// SPDX-License-Identifier: AGPL-3.0-or-later

//! Generates and maintains Ed25519 key pairs with the secret and public (PublicKey) counterparts.
pub mod error;
mod key_pair;
mod public_key;

pub use key_pair::KeyPair;
pub use public_key::PublicKey;
