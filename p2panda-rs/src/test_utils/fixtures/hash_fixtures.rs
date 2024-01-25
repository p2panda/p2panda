// SPDX-License-Identifier: AGPL-3.0-or-later

use rand::Rng;
use rstest::fixture;

use crate::{hash::Hash, test_utils::constants::HASH};

/// Returns constant testing HASH.
#[fixture]
pub fn hash(#[default(HASH)] hash_str: &str) -> Hash {
    hash_str.parse().unwrap()
}

/// Fixture which injects a random hash into a test method.
#[fixture]
pub fn random_hash() -> Hash {
    let random_data = rand::thread_rng().gen::<[u8; 32]>();
    Hash::new_from_bytes(&random_data)
}
