// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_encryption::Rng;
use p2panda_encryption::crypto::x25519::SecretKey;
use p2panda_encryption::key_bundle::Lifetime;
use p2panda_encryption::key_manager::KeyManager;

use crate::key_secrets::traits::KeySecretsStore;
use crate::{SqliteStore, tx_unwrap};

#[tokio::test]
async fn set_get_pre_key_secret() {
    let store = SqliteStore::temporary().await;

    // Initialise key manager with prekey bundles.
    let rng = Rng::from_seed([7; 32]);
    let lifetime = Lifetime::default();
    let identity_secret = SecretKey::from_rng(&rng).unwrap();
    let key_manager =
        KeyManager::init_and_generate_prekey(&identity_secret, lifetime, &rng).unwrap();

    // Retrieve prekey bundles; this is what we're going to store.
    let state = key_manager.prekey_bundles();

    // Store should be empty to start with.
    assert!(
        <SqliteStore as KeySecretsStore>::get_prekey_secrets(&store)
            .await
            .unwrap()
            .is_none()
    );

    // Store the prekey bundles.
    tx_unwrap!(store, store.set_prekey_secrets(state).await.unwrap());

    // Prekey bundles are successfully retrieved from the store.
    assert_eq!(
        store.get_prekey_secrets().await.unwrap(),
        Some(state.clone())
    );

    // Initialise a second key manager so we have a new (unique) set of prekey bundles.
    let key_manager =
        KeyManager::init_and_generate_prekey(&identity_secret, lifetime, &rng).unwrap();
    let new_state = key_manager.prekey_bundles();

    // Ensure the prekey bundles are unique.
    assert_ne!(state, new_state);

    // Store the new prekey bundles.
    tx_unwrap!(store, store.set_prekey_secrets(new_state).await.unwrap());

    // New prekey bundles have overwritten the previous state.
    assert_eq!(
        store.get_prekey_secrets().await.unwrap(),
        Some(new_state.clone())
    );
}
