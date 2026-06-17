// SPDX-License-Identifier: MIT OR Apache-2.0

mod forge;
mod store;

use std::borrow::Borrow;

use p2panda_auth::traits::Conditions;
use p2panda_encryption::Rng;
use p2panda_store::SqliteError;
use p2panda_store::SqliteStore;
use p2panda_store::operations::OperationStore;
use p2panda_store::tx_unwrap;
use serde::Deserialize;
use serde::Serialize;

use crate::Config;
use crate::Credentials;
use crate::SpacesArgs;
use crate::manager::Manager;
use crate::space::SpaceError;
use crate::test_utils::forge::DEFAULT_LOG_ID;
use crate::traits::AuthoredMessage;
use crate::traits::SpaceId;
use crate::types::StrongRemoveResolver;

pub use forge::TestForge;
pub use store::TestKeyStore;

pub type TestSpaceId = usize;

impl SpaceId for TestSpaceId {}

pub type TestPeerId = u8;

#[derive(Clone, Debug, PartialEq, PartialOrd, Deserialize, Serialize)]
pub struct TestConditions {}

impl Conditions for TestConditions {}

// Extension type defined in p2panda.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SpacesExtensions {
    args: SpacesArgs<TestSpaceId, TestConditions>,
}

// Required Borrow<SpacesArgs> will be implemented in p2panda.
impl Borrow<SpacesArgs<TestSpaceId, TestConditions>> for SpacesExtensions {
    fn borrow(&self) -> &SpacesArgs<TestSpaceId, TestConditions> {
        &self.args
    }
}

// Required Borrow<SpacesArgs> will be implemented in p2panda.
impl Borrow<SpacesArgs<TestSpaceId, TestConditions>> for Operation {
    fn borrow(&self) -> &SpacesArgs<TestSpaceId, TestConditions> {
        &self.header().extensions.args
    }
}

impl AuthoredMessage for Operation {
    fn id(&self) -> crate::OperationId {
        self.hash
    }

    fn author(&self) -> crate::ActorId {
        self.header().verifying_key
    }
}

pub type Operation = p2panda_core::Operation<SpacesExtensions>;
pub type SqliteSpacesStore = p2panda_store::spaces::SqliteSpacesStore<Operation>;

pub type TestManager = Manager<
    TestSpaceId,
    SqliteSpacesStore,
    TestKeyStore,
    TestForge,
    TestConditions,
    StrongRemoveResolver<TestConditions>,
>;

pub type TestSpaceError = SpaceError<
    TestSpaceId,
    TestKeyStore,
    TestForge,
    TestConditions,
    StrongRemoveResolver<TestConditions>,
>;

pub struct TestPeer {
    #[allow(unused)]
    pub(crate) id: TestPeerId,
    #[allow(unused)]
    pub(crate) manager: TestManager,
    #[allow(unused)]
    pub(crate) credentials: Credentials,
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
        let spaces_store = SqliteSpacesStore::new(store.clone());
        let key_store = TestKeyStore::new();
        let forge = TestForge::new(store, credentials.signing_key());

        let manager = TestManager::new_with_config(
            spaces_store,
            key_store,
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
        }
    }

    pub async fn persist_operation(&self, operation: &Operation) -> Result<bool, SqliteError> {
        let manager = self.manager.inner.write().await;
        tx_unwrap!(manager.store.inner(), {
            manager
                .store
                .inner()
                .insert_operation(&operation.hash, operation, &DEFAULT_LOG_ID)
                .await
        })
    }
}
