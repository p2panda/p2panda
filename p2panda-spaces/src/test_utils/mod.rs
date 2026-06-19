// SPDX-License-Identifier: MIT OR Apache-2.0

mod forge;

use std::borrow::Borrow;

use p2panda_encryption::Rng;
use p2panda_store::operations::OperationStore;
use p2panda_store::spaces::SqliteSpacesStore;
use p2panda_store::{SqliteError, SqliteStore, tx_unwrap};

use crate::manager::Manager;
use crate::space::SpaceError;
use crate::test_utils::forge::DEFAULT_LOG_ID;
use crate::types::StrongRemoveResolver;
use crate::{Config, Credentials, SpacesArgs};

pub use forge::TestForge;

pub type TestPeerId = u8;

pub type TestConditions = ();

pub type TestExtensions = SpacesArgs<TestConditions>;

pub type TestOperation = p2panda_core::Operation<TestExtensions>;

pub type TestSpacesStore = p2panda_store::spaces::SqliteSpacesStore<TestExtensions>;

impl Borrow<SpacesArgs<TestConditions>> for TestOperation {
    fn borrow(&self) -> &SpacesArgs<TestConditions> {
        &self.header.extensions
    }
}

pub type TestManager = Manager<
    SqliteSpacesStore<TestExtensions>,
    TestForge,
    TestConditions,
    StrongRemoveResolver<TestConditions>,
>;

pub type TestSpaceError =
    SpaceError<TestForge, TestConditions, StrongRemoveResolver<TestConditions>>;

pub struct TestPeer {
    pub id: TestPeerId,
    pub manager: TestManager,
    pub credentials: Credentials,
    pub store: SqliteStore,
}

impl TestPeer {
    pub async fn new(peer_id: TestPeerId) -> Self {
        let rng = Rng::from_seed([peer_id; 32]);
        let credentials = Credentials::from_rng(&rng).unwrap();
        let config = Config::default();
        Self::new_with_config(peer_id, credentials, &config, rng).await
    }

    pub async fn new_with_config(
        peer_id: TestPeerId,
        credentials: Credentials,
        config: &Config,
        rng: Rng,
    ) -> Self {
        let store = SqliteStore::temporary().await;
        let spaces_store = TestSpacesStore::new(store.clone());
        let forge = TestForge::new(store.clone(), credentials.signing_key());

        let manager = TestManager::new_with_config(
            spaces_store.clone(),
            forge,
            credentials.clone(),
            config,
            rng,
        )
        .await
        .unwrap();

        Self {
            id: peer_id,
            manager,
            credentials,
            store,
        }
    }

    pub async fn persist_operation(&self, operation: &TestOperation) -> Result<bool, SqliteError> {
        tx_unwrap!(self.store, {
            self.store
                .insert_operation(&operation.hash, operation, &DEFAULT_LOG_ID)
                .await
        })
    }
}
