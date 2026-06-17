// SPDX-License-Identifier: MIT OR Apache-2.0

use std::convert::Infallible;
use std::sync::Arc;

use p2panda_encryption::key_manager::PreKeyBundlesState;
use p2panda_encryption::key_registry::{KeyRegistry, KeyRegistryState};
use p2panda_store::key_registry::KeyRegistryStore;
use p2panda_store::key_secrets::KeySecretsStore;
use tokio::sync::RwLock;

use crate::types::ActorId;

#[derive(Clone, Debug)]
pub struct TestKeyStore {
    pub(crate) inner: Arc<RwLock<TestKeyStoreInner>>,
}

#[derive(Debug)]
pub struct TestKeyStoreInner {
    prekey_secrets: PreKeyBundlesState,
    key_registry: KeyRegistryState<ActorId>,
}

impl Default for TestKeyStore {
    fn default() -> Self {
        Self::new()
    }
}

impl TestKeyStore {
    pub fn new() -> Self {
        let inner = TestKeyStoreInner {
            prekey_secrets: PreKeyBundlesState::default(),
            key_registry: KeyRegistry::init(),
        };
        Self {
            inner: Arc::new(RwLock::new(inner)),
        }
    }
}

impl KeyRegistryStore<KeyRegistryState<ActorId>> for TestKeyStore {
    type Error = Infallible;

    async fn get_key_registry(&self) -> Result<Option<KeyRegistryState<ActorId>>, Self::Error> {
        let inner = self.inner.read().await;
        Ok(Some(inner.key_registry.clone()))
    }

    async fn set_key_registry(&self, y: &KeyRegistryState<ActorId>) -> Result<(), Self::Error> {
        let mut inner = self.inner.write().await;
        inner.key_registry = y.clone();
        Ok(())
    }
}

impl KeySecretsStore<PreKeyBundlesState> for TestKeyStore {
    type Error = Infallible;

    async fn get_prekey_secrets(&self) -> Result<Option<PreKeyBundlesState>, Self::Error> {
        let inner = self.inner.read().await;
        Ok(Some(inner.prekey_secrets.clone()))
    }

    async fn set_prekey_secrets(&self, y: &PreKeyBundlesState) -> Result<(), Self::Error> {
        let mut inner = self.inner.write().await;
        inner.prekey_secrets = y.clone();
        Ok(())
    }
}
