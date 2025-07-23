// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashMap;

use thiserror::Error;

use crate::encryption::{KeyManagerState, KeyRegistryState};
use crate::space::SpaceState;
use crate::store::{KeyStore, SpaceStore};
use crate::types::{ActorId, Conditions};

#[derive(Debug)]
pub struct MemoryStore<M, C, RS>
where
    C: Conditions,
{
    key_manager: KeyManagerState,
    key_registry: KeyRegistryState,
    spaces: HashMap<ActorId, SpaceState<M, C, RS>>,
}

impl<M, C, RS> MemoryStore<M, C, RS>
where
    C: Conditions,
{
    pub fn new() -> Self {
        Self {
            key_manager: todo!(),
            key_registry: todo!(),
            spaces: HashMap::new(),
        }
    }
}

impl<M, C, RS> SpaceStore<M, C, RS> for MemoryStore<M, C, RS>
where
    C: Conditions,
{
    type Error = MemoryStoreError;

    async fn space(&self, id: ActorId) -> Result<SpaceState<M, C, RS>, Self::Error> {
        todo!()
    }

    async fn set_space(
        &mut self,
        id: ActorId,
        y: SpaceState<M, C, RS>,
    ) -> Result<SpaceState<M, C, RS>, Self::Error> {
        todo!()
    }
}

impl<M, C, RS> KeyStore for MemoryStore<M, C, RS>
where
    C: Conditions,
{
    type Error = MemoryStoreError;

    async fn key_manager(&self) -> Result<KeyManagerState, Self::Error> {
        todo!()
    }

    async fn key_registry(&self) -> Result<KeyRegistryState, Self::Error> {
        todo!()
    }

    async fn set_key_manager(&mut self, y: &KeyManagerState) -> Result<(), Self::Error> {
        todo!()
    }

    async fn set_key_registry(&mut self, y: &KeyRegistryState) -> Result<(), Self::Error> {
        todo!()
    }
}

#[derive(Debug, Error)]
pub enum MemoryStoreError {
    #[error("tried to access unknown space with id {0}")]
    UnknownSpace(ActorId),
}
