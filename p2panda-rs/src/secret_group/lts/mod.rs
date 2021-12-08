// SPDX-License-Identifier: AGPL-3.0-or-later

//! Structs and methods to handle p2panda Long Term Secrets.
//!
//! Long Term Secrets contain symmetric AEAD keys which are used to en- & decrypt data over longer
//! periods of time, spanning over multiple MLS group epochs.
mod aead;
mod ciphersuite;
mod ciphertext;
mod constants;
mod epoch;
mod error;
mod nonce;
mod secret;

pub use ciphersuite::LongTermSecretCiphersuite;
pub use ciphertext::LongTermSecretCiphertext;
pub use constants::*;
pub use epoch::LongTermSecretEpoch;
pub use error::LongTermSecretError;
pub use nonce::LongTermSecretNonce;
pub use secret::LongTermSecret;
