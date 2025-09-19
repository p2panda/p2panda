// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashMap;
use std::convert::Infallible;
use std::hash::Hash;

use p2panda_auth::traits::Conditions;
use p2panda_encryption::key_manager::{KeyManager, KeyManagerState};
use p2panda_encryption::key_registry::{KeyRegistry, KeyRegistryState};
use p2panda_encryption::traits::PreKeyManager;

use crate::OperationId;
use crate::space::SpaceState;
use crate::store::{AuthStore, KeyStore, MessageStore, SpaceStore};
use crate::traits::SpaceId;
use crate::types::{ActorId, AuthGroupState};

#[derive(Debug)]
pub struct MemoryStore<ID, M, C>
where
    C: Conditions,
{
    key_manager: KeyManagerState,
    key_registry: KeyRegistryState<ActorId>,
    auth: AuthGroupState<C>,
    spaces: HashMap<ID, SpaceState<ID, M, C>>,
    messages: HashMap<OperationId, M>,
}

impl<ID, M, C> MemoryStore<ID, M, C>
where
    ID: SpaceId,
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
            messages: HashMap::new(),
        }
    }
}

impl<ID, M, C> SpaceStore<ID, M, C> for MemoryStore<ID, M, C>
where
    ID: SpaceId + Hash,
    M: Clone,
    C: Conditions,
{
    type Error = Infallible;

    async fn space(&self, id: &ID) -> Result<Option<SpaceState<ID, M, C>>, Self::Error> {
        Ok(self.spaces.get(id).cloned())
    }

    async fn has_space(&self, id: &ID) -> Result<bool, Self::Error> {
        Ok(self.spaces.contains_key(id))
    }

    async fn spaces(&self) -> Result<Vec<ID>, Self::Error> {
        Ok(self.spaces.keys().cloned().collect())
    }

    async fn set_space(&mut self, id: &ID, y: SpaceState<ID, M, C>) -> Result<(), Self::Error> {
        self.spaces.insert(*id, y);
        Ok(())
    }
}

impl<ID, M, C> KeyStore for MemoryStore<ID, M, C>
where
    ID: SpaceId,
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

impl<ID, M, C> AuthStore<C> for MemoryStore<ID, M, C>
where
    ID: SpaceId,
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

impl<ID, M, C> MessageStore<M> for MemoryStore<ID, M, C>
where
    ID: SpaceId,
    M: Clone,
    C: Conditions,
{
    type Error = Infallible;

    async fn message(&self, id: &OperationId) -> Result<Option<M>, Self::Error> {
        Ok(self.messages.get(id).cloned())
    }

    async fn set_message(&mut self, id: &OperationId, message: &M) -> Result<(), Self::Error> {
        self.messages.insert(*id, message.clone());
        Ok(())
    }
}
