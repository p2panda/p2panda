// SPDX-License-Identifier: AGPL-3.0-or-later

use rstest::fixture;

use crate::identity::{KeyPair, PublicKey};
use crate::test_utils::constants::PRIVATE_KEY;

/// Fixture which injects the default private key string into a test method.
#[fixture]
pub fn private_key() -> String {
    PRIVATE_KEY.into()
}

/// Fixture which injects the default public key into a test method.
#[fixture]
pub fn public_key() -> PublicKey {
    let key_pair = KeyPair::from_private_key_str(PRIVATE_KEY).unwrap();
    key_pair.public_key().to_owned()
}

/// Fixture which injects the default KeyPair into a test method. Default value can be overridden
/// at testing time by passing in a custom private key string.
#[fixture]
pub fn key_pair(#[default(PRIVATE_KEY)] private_key: &str) -> KeyPair {
    KeyPair::from_private_key_str(private_key).unwrap()
}

/// Fixture which injects a random KeyPair into a test method.
#[fixture]
pub fn random_key_pair() -> KeyPair {
    KeyPair::new()
}
