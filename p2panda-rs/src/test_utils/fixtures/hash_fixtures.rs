// SPDX-License-Identifier: AGPL-3.0-or-later

use rand::Rng;
use rstest::fixture;

use crate::hash::Hash;

/// Fixture which injects a random hash into a test method.
#[fixture]
pub fn random_hash() -> Hash {
    let random_data = rand::thread_rng().gen::<[u8; 32]>().to_vec();
    Hash::new_from_bytes(random_data)
}
