// SPDX-License-Identifier: MIT OR Apache-2.0

use std::borrow::Borrow;

use p2panda_core::Hash;
use serde::{Deserialize, Serialize};

use crate::spaces::{SpacesMessageStore, SpacesStore, SpacesStoreWrite};
use crate::{SqliteStore, tx_unwrap};

// Additional message arguments required by and defined in p2panda-spaces.
#[derive(Clone, Debug, Serialize, Deserialize)]
struct SpacesArgs;

// Here we fix the generic arguments on SpacesMessage, this would happen in p2panda-spaces.
type SpacesMessage = crate::spaces::SpacesMessage<SpacesArgs>;

// Extension type defined in p2panda.
#[derive(Clone, Debug, Serialize, Deserialize)]
struct SpacesExtensions {
    args: SpacesArgs,
}

// Required Borrow<SpacesArgs> will be implemented in p2panda.
impl Borrow<SpacesArgs> for SpacesExtensions {
    fn borrow(&self) -> &SpacesArgs {
        &self.args
    }
}

type SqliteSpacesStore = crate::spaces::SqliteSpacesStore<SpacesExtensions>;

type SpaceState = String;

#[tokio::test]
async fn verify_generics() {
    // Construct concrete SqliteSpacesStore which bounds the persisted message type to
    // `Operation<SpacesExtensions>`.
    let inner = SqliteStore::temporary().await;
    let store = SqliteSpacesStore::new(inner);

    // We can query the store now.
    let id = Hash::from_bytes([0; 32]);
    let message: Option<SpacesMessage> = store.get_spaces_message(&id).await.unwrap();

    // Although there are no operations inserted so we expect None.
    assert!(message.is_none());
}

#[tokio::test]
async fn get_set_spaces_state() {
    let inner = SqliteStore::temporary().await;
    let store = SqliteSpacesStore::new(inner);

    let space_state = String::from("Some important state");
    let space_id = Hash::digest(b"test");

    tx_unwrap!(store, {
        store.set_space_state_tx(&space_id, &space_state).await
    })
    .unwrap();

    let y: Option<SpaceState> =
        tx_unwrap!(store, { store.get_space_state_tx(&space_id).await }).unwrap();

    assert!(y.is_some());
    let y = y.unwrap();
    assert_eq!(y, space_state);

    let has_space = <SqliteSpacesStore as SpacesStore<SpaceState>>::has_space(&store, &space_id)
        .await
        .unwrap();
    assert!(has_space);

    let non_existent_space_id = Hash::digest(b"nonono");
    let y: Option<SpaceState> = tx_unwrap!(store, {
        store.get_space_state_tx(&non_existent_space_id).await
    })
    .unwrap();
    assert!(y.is_none());

    let has_space =
        <SqliteSpacesStore as SpacesStore<SpaceState>>::has_space(&store, &non_existent_space_id)
            .await
            .unwrap();
    assert!(!has_space);
}

#[tokio::test]
async fn get_space_ids() {
    let inner = SqliteStore::temporary().await;
    let store = SqliteSpacesStore::new(inner);

    let space_id_0 = Hash::digest(b"0");
    let space_id_1 = Hash::digest(b"1");
    let space_id_2 = Hash::digest(b"2");

    let space_state = String::from("Some important state");
    tx_unwrap!(store, {
        store.set_space_state_tx(&space_id_0, &space_state).await
    })
    .unwrap();
    tx_unwrap!(store, {
        store.set_space_state_tx(&space_id_1, &space_state).await
    })
    .unwrap();
    tx_unwrap!(store, {
        store.set_space_state_tx(&space_id_2, &space_state).await
    })
    .unwrap();

    let space_ids: Vec<Hash> = <SqliteSpacesStore as SpacesStore<SpaceState>>::space_ids(&store)
        .await
        .unwrap();

    assert_eq!(space_ids.len(), 3);
    assert!(space_ids.contains(&space_id_0));
    assert!(space_ids.contains(&space_id_1));
    assert!(space_ids.contains(&space_id_2));
}
