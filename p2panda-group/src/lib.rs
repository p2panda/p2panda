// SPDX-License-Identifier: MIT OR Apache-2.0

// TODO: To be removed.
#![allow(dead_code)]
mod crypto;
mod data_scheme;
mod key_bundle;
mod key_manager;
mod key_registry;
mod message_scheme;
pub mod traits;
mod two_party;

pub use crypto::{Rng, RngError};
pub use key_bundle::{
    Lifetime, LifetimeError, LongTermKeyBundle, OneTimeKeyBundle, OneTimePreKey, OneTimePreKeyId,
};
pub use key_manager::{KeyManager, KeyManagerError, KeyManagerState};
pub use key_registry::{KeyRegistry, KeyRegistryState};
pub use two_party::{
    LongTermTwoParty, OneTimeTwoParty, TwoParty, TwoPartyCiphertext, TwoPartyError, TwoPartyMessage,
};

#[cfg(feature = "test_utils")]
pub mod test_utils {
    pub use crate::crypto::x25519::SecretKey;
}
