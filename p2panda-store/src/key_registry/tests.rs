// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_core::SigningKey;
use p2panda_encryption::Rng;
use p2panda_encryption::crypto::x25519::SecretKey;
use p2panda_encryption::key_bundle::{Lifetime, LongTermKeyBundle, PreKey};
use p2panda_encryption::key_registry::KeyRegistry;
use p2panda_spaces::ActorId;

use crate::key_registry::traits::KeyRegistryStore;
use crate::{SqliteStore, tx_unwrap};

#[tokio::test]
async fn set_get_key_registry() {
    let store = SqliteStore::temporary().await;

    let rng = Rng::from_seed([1; 32]);

    let signing_key = SigningKey::generate();
    let verifying_key = signing_key.verifying_key();

    let member_id: ActorId = verifying_key.to_hex().parse().unwrap();
    let identity_secret = SecretKey::from_bytes(rng.random_array().unwrap());

    // Generate the first prekey bundle.
    let bundle_1 = {
        let prekey_secret = SecretKey::from_bytes(rng.random_array().unwrap());
        let prekey = PreKey::new(prekey_secret.verifying_key().unwrap(), Lifetime::new(120));
        let prekey_signature = prekey.sign(&identity_secret, &rng).unwrap();

        LongTermKeyBundle::new(
            identity_secret.verifying_key().unwrap(),
            prekey,
            prekey_signature,
        )
    };

    // Initialize key registry and register bundles there.
    let state = KeyRegistry::init();
    let state = KeyRegistry::add_longterm_bundle(state, member_id, bundle_1.clone()).unwrap();

    // Store should be empty to start with.
    assert!(store.get_key_registry().await.unwrap().is_none());

    // Store the key registry.
    tx_unwrap!(store, store.set_key_registry(&state).await.unwrap());

    // Key registry state successfully retrieved from the store.
    assert_eq!(store.get_key_registry().await.unwrap(), Some(state.clone()));

    // Generate the second prekey bundle.
    let bundle_2 = {
        let prekey_secret = SecretKey::from_bytes(rng.random_array().unwrap());
        let prekey = PreKey::new(prekey_secret.verifying_key().unwrap(), Lifetime::new(60));
        let prekey_signature = prekey.sign(&identity_secret, &rng).unwrap();

        LongTermKeyBundle::new(
            identity_secret.verifying_key().unwrap(),
            prekey,
            prekey_signature,
        )
    };

    // Update the key registry.
    let new_state =
        KeyRegistry::add_longterm_bundle(state.clone(), member_id, bundle_2.clone()).unwrap();

    // Ensure the key registy states are unique.
    assert_ne!(state, new_state);

    // Store the updated key registry state.
    tx_unwrap!(store, store.set_key_registry(&new_state).await.unwrap());

    // New key registry state has overwritten the previous state.
    assert_eq!(
        store.get_key_registry().await.unwrap(),
        Some(new_state.clone())
    );
}
