// SPDX-License-Identifier: MIT OR Apache-2.0

use std::sync::Arc;
use std::{convert::Infallible, marker::PhantomData};

use p2panda_core::{PrivateKey, PublicKey};
use p2panda_encryption::Rng;
use p2panda_encryption::key_manager::{KeyManager, KeyManagerError, KeyManagerState};
use p2panda_encryption::key_registry::{KeyRegistry, KeyRegistryState};
use tokio::sync::RwLock;

use crate::message::SpacesArgs;
use crate::test_utils::{SeqNum, TestConditions, TestMessage};
use crate::traits::SpaceId;
use crate::traits::key_store::{Forge, KeyManagerStore, KeyRegistryStore};
use crate::types::ActorId;
use crate::{Config, Credentials};

#[derive(Debug)]
pub struct TestKeyStoreInner<ID> {
    next_seq_num: SeqNum,
    private_key: PrivateKey,
    key_manager: KeyManagerState,
    key_registry: KeyRegistryState<ActorId>,
    _phantom: PhantomData<ID>,
}

#[derive(Debug)]
pub struct TestKeyStore<ID> {
    pub(crate) public_key: PublicKey,
    pub(crate) inner: Arc<RwLock<TestKeyStoreInner<ID>>>,
}

impl<ID> TestKeyStore<ID> {
    pub fn new(
        credentials: &Credentials,
        config: &Config,
        rng: &Rng,
    ) -> Result<Self, KeyManagerError> {
        let key_manager = KeyManager::init(&credentials.identity_secret(), config.lifetime(), rng)?;
        let key_registry = KeyRegistry::init();
        let public_key = credentials.public_key();
        let inner = TestKeyStoreInner {
            next_seq_num: 0,
            private_key: credentials.private_key(),
            key_manager,
            key_registry,
            _phantom: PhantomData,
        };
        Ok(Self {
            public_key,
            inner: Arc::new(RwLock::new(inner)),
        })
    }
}

impl<ID> KeyRegistryStore for TestKeyStore<ID>
where
    ID: SpaceId,
{
    type Error = Infallible;

    async fn key_registry(&self) -> Result<KeyRegistryState<ActorId>, Self::Error> {
        let inner = self.inner.read().await;
        Ok(inner.key_registry.clone())
    }

    async fn set_key_registry(&self, y: &KeyRegistryState<ActorId>) -> Result<(), Self::Error> {
        let mut inner = self.inner.write().await;
        inner.key_registry = y.clone();
        Ok(())
    }
}

impl<ID> KeyManagerStore for TestKeyStore<ID>
where
    ID: SpaceId,
{
    type Error = Infallible;

    async fn key_manager(&self) -> Result<KeyManagerState, Self::Error> {
        let inner = self.inner.read().await;
        Ok(inner.key_manager.clone())
    }

    async fn set_key_manager(&self, y: &KeyManagerState) -> Result<(), Self::Error> {
        let mut inner = self.inner.write().await;
        inner.key_manager = y.clone();
        Ok(())
    }
}

impl<ID> Forge<ID, TestMessage<ID>, TestConditions> for TestKeyStore<ID>
where
    ID: SpaceId,
{
    type Error = Infallible;

    fn public_key(&self) -> PublicKey {
        self.public_key.clone()
    }

    async fn forge(
        &self,
        args: SpacesArgs<ID, TestConditions>,
    ) -> Result<TestMessage<ID>, Self::Error> {
        let mut inner = self.inner.write().await;
        let seq_num = inner.next_seq_num;
        inner.next_seq_num += 1;
        Ok(TestMessage {
            seq_num,
            public_key: self.public_key.clone(),
            spaces_args: args,
        })
    }
}
