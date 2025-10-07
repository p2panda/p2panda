// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashMap;
use std::convert::Infallible;
use std::hash::Hash;
use std::sync::Arc;

use p2panda_auth::traits::Conditions;
use tokio::sync::RwLock;

use crate::OperationId;
use crate::auth::orderer::AuthOrderer;
use crate::space::SpaceState;
use crate::test_utils::{TestConditions, TestMessage};
use crate::traits::SpaceId;
use crate::traits::spaces_store::{AuthStore, MessageStore, SpaceStore};
use crate::types::AuthGroupState;

pub type TestSpacesStore<ID> = MemoryStore<ID, TestMessage<ID>, TestConditions>;

#[derive(Debug)]
pub struct MemoryStoreInner<ID, M, C>
where
    C: Conditions,
{
    auth: AuthGroupState<C>,
    spaces: HashMap<ID, SpaceState<ID, M, C>>,
    messages: HashMap<OperationId, M>,
}

#[derive(Debug, Clone)]
pub struct MemoryStore<ID, M, C>
where
    C: Conditions,
{
    pub(crate) inner: Arc<RwLock<MemoryStoreInner<ID, M, C>>>,
}

impl<ID, M, C> MemoryStore<ID, M, C>
where
    ID: SpaceId,
    C: Conditions,
{
    pub fn new() -> Self {
        let orderer_y = AuthOrderer::init();
        let auth_y = AuthGroupState::new(orderer_y);
        let inner = MemoryStoreInner {
            auth: auth_y,
            spaces: HashMap::new(),
            messages: HashMap::new(),
        };
        Self {
            inner: Arc::new(RwLock::new(inner)),
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
        let inner = self.inner.read().await;
        Ok(inner.spaces.get(id).cloned())
    }

    async fn has_space(&self, id: &ID) -> Result<bool, Self::Error> {
        let inner = self.inner.read().await;
        Ok(inner.spaces.contains_key(id))
    }

    async fn spaces_ids(&self) -> Result<Vec<ID>, Self::Error> {
        let inner = self.inner.read().await;
        Ok(inner.spaces.keys().cloned().collect())
    }

    async fn set_space(&mut self, id: &ID, y: SpaceState<ID, M, C>) -> Result<(), Self::Error> {
        let mut inner = self.inner.write().await;
        inner.spaces.insert(*id, y);
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
        let inner = self.inner.read().await;
        Ok(inner.auth.clone())
    }

    async fn set_auth(&mut self, y: &AuthGroupState<C>) -> Result<(), Self::Error> {
        let mut inner = self.inner.write().await;
        inner.auth = y.clone();
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
        let inner = self.inner.read().await;
        Ok(inner.messages.get(id).cloned())
    }

    async fn set_message(&mut self, id: &OperationId, message: &M) -> Result<(), Self::Error> {
        let mut inner = self.inner.write().await;
        inner.messages.insert(*id, message.clone());
        Ok(())
    }
}
