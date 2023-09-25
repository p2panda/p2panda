// SPDX-License-Identifier: AGPL-3.0-or-later

use rand::Rng;
use rstest::fixture;

use crate::hash_v2::Hash;

/// Fixture which injects a random hash into a test method.
#[fixture]
pub fn random_hash() -> Hash {
    let random_data = rand::thread_rng().gen::<[u8; 32]>();
    Hash::new_from_bytes(&random_data)
}
