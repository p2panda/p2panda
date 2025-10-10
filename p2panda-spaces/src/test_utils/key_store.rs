// SPDX-License-Identifier: MIT OR Apache-2.0

use std::convert::Infallible;
use std::sync::Arc;

use p2panda_core::{PrivateKey, PublicKey};
use p2panda_encryption::key_manager::{KeyManagerError, PreKeyBundlesState};
use p2panda_encryption::key_registry::{KeyRegistry, KeyRegistryState};
use tokio::sync::RwLock;

use crate::Credentials;
use crate::message::SpacesArgs;
use crate::test_utils::{SeqNum, TestConditions, TestMessage, TestSpacesStore};
use crate::traits::SpaceId;
use crate::traits::key_store::{Forge, KeyRegistryStore, KeySecretStore};
use crate::traits::message::AuthoredMessage;
use crate::traits::spaces_store::MessageStore;
use crate::types::ActorId;

#[derive(Debug)]
pub struct TestKeyStoreInner<ID> {
    next_seq_num: SeqNum,
    private_key: PrivateKey,
    prekey_secrets: PreKeyBundlesState,
    key_registry: KeyRegistryState<ActorId>,
    spaces_store: TestSpacesStore<ID>,
}

#[derive(Debug)]
pub struct TestKeyStore<ID> {
    pub(crate) public_key: PublicKey,
    pub(crate) inner: Arc<RwLock<TestKeyStoreInner<ID>>>,
}

impl<ID> TestKeyStore<ID> {
    pub fn new(
        spaces_store: TestSpacesStore<ID>,
        credentials: &Credentials,
    ) -> Result<Self, KeyManagerError> {
        let public_key = credentials.public_key();
        let inner = TestKeyStoreInner {
            next_seq_num: 0,
            private_key: credentials.private_key(),
            prekey_secrets: PreKeyBundlesState::default(),
            key_registry: KeyRegistry::init(),
            spaces_store,
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

impl<ID> KeySecretStore for TestKeyStore<ID>
where
    ID: SpaceId,
{
    type Error = Infallible;

    async fn prekey_secrets(&self) -> Result<PreKeyBundlesState, Self::Error> {
        let inner = self.inner.read().await;
        Ok(inner.prekey_secrets.clone())
    }

    async fn set_prekey_secrets(&self, y: &PreKeyBundlesState) -> Result<(), Self::Error> {
        let mut inner = self.inner.write().await;
        inner.prekey_secrets = y.clone();
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
        let message = TestMessage {
            seq_num,
            public_key: self.public_key.clone(),
            spaces_args: args,
        };
        inner
            .spaces_store
            .set_message(&message.id(), &message)
            .await
            .unwrap();
        Ok(message)
    }
}
