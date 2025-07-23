// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashMap;

use p2panda_encryption::key_manager::{KeyManager, KeyManagerState};
use p2panda_encryption::key_registry::{KeyRegistry, KeyRegistryState};
use p2panda_encryption::traits::PreKeyManager;
use thiserror::Error;

use crate::space::SpaceState;
use crate::store::{KeyStore, SpaceStore};
use crate::types::{ActorId, Conditions};

#[derive(Debug)]
pub struct MemoryStore<M, C, RS>
where
    C: Conditions,
{
    key_manager: KeyManagerState,
    key_registry: KeyRegistryState<ActorId>,
    spaces: HashMap<ActorId, SpaceState<M, C, RS>>,
}

impl<M, C, RS> MemoryStore<M, C, RS>
where
    C: Conditions,
{
    pub fn new(my_id: ActorId, key_manager: KeyManagerState) -> Self {
        // Register our own pre-keys.
        let key_registry = {
            let key_bundle = KeyManager::prekey_bundle(&key_manager);
            let y = KeyRegistry::init();
            KeyRegistry::add_longterm_bundle(y, my_id, key_bundle)
        };

        Self {
            key_manager,
            key_registry,
            spaces: HashMap::new(),
        }
    }
}

impl<M, C, RS> SpaceStore<M, C, RS> for MemoryStore<M, C, RS>
where
    M: Clone,
    RS: Clone,
    C: Conditions,
{
    type Error = MemoryStoreError;

    async fn space(&self, id: &ActorId) -> Result<SpaceState<M, C, RS>, Self::Error> {
        self.spaces
            .get(&id)
            .ok_or(MemoryStoreError::UnknownSpace(*id))
            .cloned()
    }

    async fn set_space(
        &mut self,
        id: &ActorId,
        y: SpaceState<M, C, RS>,
    ) -> Result<(), Self::Error> {
        self.spaces.insert(*id, y);
        Ok(())
    }
}

impl<M, C, RS> KeyStore for MemoryStore<M, C, RS>
where
    C: Conditions,
{
    type Error = MemoryStoreError;

    async fn key_manager(&self) -> Result<KeyManagerState, Self::Error> {
        Ok(self.key_manager.clone())
    }

    async fn key_registry(&self) -> Result<KeyRegistryState<ActorId>, Self::Error> {
        Ok(self.key_registry.clone())
    }

    async fn set_key_manager(&mut self, y: &KeyManagerState) -> Result<(), Self::Error> {
        self.key_manager = y.clone();
        Ok(())
    }

    async fn set_key_registry(&mut self, y: &KeyRegistryState<ActorId>) -> Result<(), Self::Error> {
        self.key_registry = y.clone();
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum MemoryStoreError {
    #[error("tried to access unknown space with id {0}")]
    UnknownSpace(ActorId),
}
