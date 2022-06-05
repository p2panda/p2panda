// SPDX-License-Identifier: AGPL-3.0-or-later

use rand::Rng;
use rstest::fixture;

use crate::hash::Hash;
use crate::test_utils::constants::DEFAULT_HASH;

/// Fixture which injects the default Hash into a test method. Default value can be overridden at
/// testing time by passing in a custom hash string.
#[fixture]
pub fn hash(#[default(DEFAULT_HASH)] hash_str: &str) -> Hash {
    Hash::new(hash_str).unwrap()
}

/// Fixture which injects a random hash into a test method.
#[fixture]
pub fn random_hash() -> Hash {
    let random_data = rand::thread_rng().gen::<[u8; 32]>().to_vec();
    Hash::new_from_bytes(random_data).unwrap()
}

/// Fixture which injects the default Hash into a test method as an Option.
///
/// Default value can be overridden at testing time by passing in custom hash string.
#[fixture]
pub fn some_hash(#[default(DEFAULT_HASH)] str: &str) -> Option<Hash> {
    let hash = Hash::new(str);
    Some(hash.unwrap())
}
