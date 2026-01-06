// SPDX-License-Identifier: MIT OR Apache-2.0

mod forge;
mod message;
mod store;

use p2panda_auth::traits::Conditions;
use p2panda_encryption::Rng;

use crate::Config;
use crate::Credentials;
use crate::manager::Manager;
use crate::space::SpaceError;
use crate::traits::SpaceId;
use crate::types::StrongRemoveResolver;

pub use forge::TestForge;
pub use message::TestMessage;
pub use store::{TestKeyStore, TestStore};

pub type TestSpaceId = usize;

impl SpaceId for TestSpaceId {}

pub type TestPeerId = u8;

#[derive(Clone, Debug, PartialEq, PartialOrd)]
pub struct TestConditions {}

impl Conditions for TestConditions {}

pub type TestManager = Manager<
    TestSpaceId,
    TestStore,
    TestKeyStore,
    TestForge<TestStore>,
    TestMessage,
    TestConditions,
    StrongRemoveResolver<TestConditions>,
>;

pub type TestSpaceError = SpaceError<
    TestSpaceId,
    TestStore,
    TestKeyStore,
    TestForge<TestStore>,
    TestMessage,
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
        let spaces_store = TestStore::new();
        let key_store = TestKeyStore::new();
        let forge = TestForge::new(spaces_store.clone(), credentials.private_key());

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
}
