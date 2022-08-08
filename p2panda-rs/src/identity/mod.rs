// SPDX-License-Identifier: AGPL-3.0-or-later

//! Generates and maintains Ed25519 key pairs with the secret and public (Author) counterparts.
mod author;
pub mod error;
mod key_pair;

pub use author::Author;
pub use key_pair::KeyPair;
