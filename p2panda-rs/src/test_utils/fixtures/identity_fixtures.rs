// SPDX-License-Identifier: AGPL-3.0-or-later

use std::convert::TryFrom;

use rstest::fixture;

use crate::identity::{Author, KeyPair};
use crate::test_utils::constants::DEFAULT_PRIVATE_KEY;

/// Fixture which injects the default private key string into a test method.
#[fixture]
pub fn private_key() -> String {
    DEFAULT_PRIVATE_KEY.into()
}

/// Fixture which injects the default author into a test method.
#[fixture]
pub fn public_key() -> Author {
    let key_pair = KeyPair::from_private_key_str(DEFAULT_PRIVATE_KEY).unwrap();
    Author::try_from(key_pair.public_key().to_owned()).unwrap()
}

/// Fixture which injects the default KeyPair into a test method. Default value can be overridden
/// at testing time by passing in a custom private key string.
#[fixture]
pub fn key_pair(#[default(DEFAULT_PRIVATE_KEY)] private_key: &str) -> KeyPair {
    KeyPair::from_private_key_str(private_key).unwrap()
}

/// Fixture which injects a random KeyPair into a test method.
#[fixture]
pub fn random_key_pair() -> KeyPair {
    KeyPair::new()
}