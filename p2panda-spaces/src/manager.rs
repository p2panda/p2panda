// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fmt::Debug;
use std::marker::PhantomData;
use std::sync::Arc;

use p2panda_auth::traits::Resolver;
use tokio::sync::RwLock;

use crate::event::Event;
use crate::group::Group;
use crate::orderer::AuthOrderer;
use crate::space::Space;
use crate::store::SpacesStore;
use crate::traits::Forge;
use crate::{ActorId, AuthDummyStore, Conditions, OperationId};

/// Create and manage spaces and groups.
///
/// Takes care of ingesting operations, updating spaces, groups and member key-material. Has access
/// to the operation and group stores, orderer, key-registry and key-manager.
///
/// Routes operations to the correct space(s), group(s) or member.
///
/// Only one instance of `Spaces` per app user.
///
/// Operations are created and published within the spaces service, reacting to arriving
/// operations, due to api calls (create group, create space), or triggered by key-bundles
/// expiring.
///
/// Users of spaces can subscribe to events which inform about member, group or space state
/// changes, application data being decrypted, pre-key bundles being published, we were added or
/// removed from a space.
///
/// Is agnostic to current p2panda-streams, networking layer, data type?
pub struct Manager<S, F, M, C, RS> {
    pub(crate) inner: Arc<RwLock<InnerManager<S, F, M, C, RS>>>,
}

pub struct InnerManager<S, F, M, C, RS> {
    pub(crate) forge: F,
    pub(crate) store: S,
    pub(crate) auth_orderer: AuthOrderer,
    _marker: PhantomData<(M, C, RS)>,
}

impl<S, F, M, C, RS> Manager<S, F, M, C, RS>
where
    S: SpacesStore,
    F: Forge<M>,
    C: Conditions,
    RS: Debug + Resolver<ActorId, OperationId, C, AuthOrderer, AuthDummyStore>,
{
    pub fn new(store: S, forge: F) -> Self {
        let auth_orderer = AuthOrderer::new();

        let inner = InnerManager {
            forge,
            store,
            auth_orderer,
            _marker: PhantomData,
        };
        Self {
            inner: Arc::new(RwLock::new(inner)),
        }
    }

    pub fn space(&self) -> Space<S, F, M, C, RS> {
        todo!()
    }

    pub async fn create_space(&self) -> Space<S, F, M, C, RS> {
        Space::create(self.clone(), Vec::new()).await
    }

    pub fn group(&self) -> Group {
        todo!()
    }

    pub fn create_group(&mut self) -> Group {
        todo!()
    }

    pub fn process(&mut self, _message: &M) -> Vec<Event<S, F, M, C, RS>> {
        todo!()
    }
}

/// Deriving clone on Manager will enforce generics to also impl Clone even though we are wrapping
/// them in an Arc. See related discussion:
/// https://stackoverflow.com/questions/72150623/deriveclone-seems-to-wrongfully-enforce-generic-to-be-clone
impl<S, F, M, C, RS> Clone for Manager<S, F, M, C, RS> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}
