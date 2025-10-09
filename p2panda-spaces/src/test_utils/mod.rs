// SPDX-License-Identifier: MIT OR Apache-2.0

mod key_store;
mod message;
mod spaces_store;

use std::hash::Hash as StdHash;

pub use key_store::TestKeyStore;
pub use message::TestMessage;
use p2panda_auth::traits::Conditions;
use p2panda_encryption::Rng;
pub use spaces_store::TestSpacesStore;

use crate::Config;
use crate::Credentials;
use crate::manager::Manager;
use crate::space::SpaceError;
use crate::traits::SpaceId;
use crate::types::StrongRemoveResolver;

type SeqNum = u64;

// Implement SpaceId for i32 which is what we use as space identifiers in the tests.
impl SpaceId for i32 {}

#[derive(Clone, Debug, PartialEq, PartialOrd)]
pub struct TestConditions {}

impl Conditions for TestConditions {}

pub type TestManager<ID> = Manager<
    ID,
    TestSpacesStore<ID>,
    TestKeyStore<ID>,
    TestMessage<ID>,
    TestConditions,
    StrongRemoveResolver<TestConditions>,
>;

pub type TestSpaceError<ID> = SpaceError<
    ID,
    TestSpacesStore<ID>,
    TestKeyStore<ID>,
    TestMessage<ID>,
    TestConditions,
    StrongRemoveResolver<TestConditions>,
>;

pub struct TestPeer<ID = i32> {
    pub(crate) id: u8,
    pub(crate) manager: TestManager<ID>,
}

impl<ID> TestPeer<ID>
where
    ID: SpaceId + StdHash,
{
    pub async fn new(peer_id: u8) -> Self {
        let rng = Rng::from_seed([peer_id; 32]);
        let credentials = Credentials::from_rng(&rng).unwrap();
        let config = Config::default();
        Self::new_with_config(peer_id, credentials, &config, rng).await
    }

    pub async fn new_with_config(
        peer_id: u8,
        credentials: Credentials,
        config: &Config,
        rng: Rng,
    ) -> Self {
        let store = TestSpacesStore::new();
        let key_store = TestKeyStore::new(store.clone(), &credentials).unwrap();
        let manager = TestManager::new_with_config(store, key_store, credentials, config, rng)
            .await
            .unwrap();

        Self {
            id: peer_id,
            manager,
        }
    }
}
