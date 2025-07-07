// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashMap;

use crate::Rng;
use crate::crypto::x25519::SecretKey;
use crate::data_scheme::dcgka::{Dcgka, DcgkaState};
use crate::data_scheme::test_utils::dgm::TestDgm;
use crate::key_bundle::Lifetime;
use crate::key_manager::KeyManager;
use crate::key_registry::KeyRegistry;
use crate::test_utils::{MemberId, MessageId};
use crate::traits::PreKeyManager;

pub type TestDcgkaState = DcgkaState<
    MemberId,
    MessageId,
    KeyRegistry<MemberId>,
    TestDgm<MemberId, MessageId>,
    KeyManager,
>;

/// Helper method returning initialised DCGKA state for each member of a test group using the "data
/// encryption" scheme.
///
/// The method will automatically generate all required long-term pre-key bundles from each member
/// and register them for each other.
pub fn init_dcgka_state<const N: usize>(
    member_ids: [MemberId; N],
    rng: &Rng,
) -> [TestDcgkaState; N] {
    let mut key_bundles = HashMap::new();
    let mut key_managers = HashMap::new();

    // Generate a pre-key bundle for each other member of the group.
    for id in member_ids {
        let identity_secret = SecretKey::from_bytes(rng.random_array().unwrap());
        let manager = KeyManager::init(&identity_secret, Lifetime::default(), rng).unwrap();

        let mut bundle_list = Vec::with_capacity(member_ids.len());
        for _ in member_ids {
            let key_bundle = KeyManager::prekey_bundle(&manager);
            bundle_list.push(key_bundle);
        }

        key_bundles.insert(id, bundle_list);
        key_managers.insert(id, manager);
    }

    // Register each other's pre-key bundles and initialise DCGKA state.
    let mut result = Vec::with_capacity(member_ids.len());
    for id in member_ids {
        let dgm = TestDgm::init(id);
        let registry = {
            let mut state = KeyRegistry::init();
            for bundle_id in member_ids {
                let bundle = key_bundles.get_mut(&bundle_id).unwrap().pop().unwrap();
                let state_i = KeyRegistry::add_longterm_bundle(state, bundle_id, bundle);
                state = state_i;
            }
            state
        };
        let manager = key_managers.remove(&id).unwrap();
        let dcgka: TestDcgkaState = Dcgka::init(id, manager, registry, dgm);
        result.push(dcgka);
    }

    result.try_into().unwrap()
}
