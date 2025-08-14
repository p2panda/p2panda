// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashMap;
use std::convert::Infallible;

use p2panda_auth::traits::Conditions;
use p2panda_encryption::key_manager::{KeyManager, KeyManagerState};
use p2panda_encryption::key_registry::{KeyRegistry, KeyRegistryState};
use p2panda_encryption::traits::PreKeyManager;

use crate::space::SpaceState;
use crate::store::{AuthStore, KeyStore, SpaceStore};
use crate::types::{ActorId, AuthGroupState};

#[derive(Debug)]
pub struct MemoryStore<M, C>
where
    C: Conditions,
{
    key_manager: KeyManagerState,
    key_registry: KeyRegistryState<ActorId>,
    auth: AuthGroupState<C>,
    spaces: HashMap<ActorId, SpaceState<M>>,
}

impl<M, C> MemoryStore<M, C>
where
    C: Conditions,
{
    pub fn new(my_id: ActorId, key_manager: KeyManagerState, auth: AuthGroupState<C>) -> Self {
        // Register our own pre-keys.
        let key_registry = {
            let key_bundle = KeyManager::prekey_bundle(&key_manager);
            let y = KeyRegistry::init();
            KeyRegistry::add_longterm_bundle(y, my_id, key_bundle)
        };

        Self {
            key_manager,
            key_registry,
            auth,
            spaces: HashMap::new(),
        }
    }
}

impl<M, C> SpaceStore<M> for MemoryStore<M, C>
where
    M: Clone,
    C: Conditions,
{
    type Error = Infallible;

    async fn space(&self, id: &ActorId) -> Result<Option<SpaceState<M>>, Self::Error> {
        Ok(self.spaces.get(id).cloned())
    }

    async fn has_space(&self, id: &ActorId) -> Result<bool, Self::Error> {
        Ok(self.spaces.contains_key(id))
    }

    async fn set_space(&mut self, id: &ActorId, y: SpaceState<M>) -> Result<(), Self::Error> {
        self.spaces.insert(*id, y);
        Ok(())
    }
}

impl<M, C> KeyStore for MemoryStore<M, C>
where
    C: Conditions,
{
    type Error = Infallible;

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

impl<M, C> AuthStore<C> for MemoryStore<M, C>
where
    C: Conditions,
{
    type Error = Infallible;

    async fn auth(&self) -> Result<AuthGroupState<C>, Self::Error> {
        Ok(self.auth.clone())
    }

    async fn set_auth(&mut self, y: &AuthGroupState<C>) -> Result<(), Self::Error> {
        self.auth = y.clone();
        Ok(())
    }
}
