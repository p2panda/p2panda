// SPDX-License-Identifier: MIT OR Apache-2.0

use std::cell::RefCell;
use std::fmt::Debug;
use std::marker::PhantomData;
use std::rc::Rc;

use p2panda_auth::traits::Resolver;
use p2panda_encryption::Rng;
use p2panda_encryption::crypto::x25519::SecretKey;
use p2panda_encryption::key_bundle::Lifetime;
use p2panda_encryption::key_manager::KeyManagerError;
use thiserror::Error;

use crate::event::Event;
use crate::group::Group;
use crate::key_manager::{KeyManager, KeyManagerState};
use crate::key_registry::{KeyRegistry, KeyRegistryState};
use crate::orderer::AuthOrderer;
use crate::space::{Space, SpaceError};
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
#[derive(Debug)]
pub struct Manager<S, F, M, C, RS> {
    pub(crate) inner: Rc<RefCell<ManagerInner<S, F, M, C, RS>>>,
}

#[derive(Debug)]
pub(crate) struct ManagerInner<S, F, M, C, RS> {
    pub(crate) forge: F,
    pub(crate) store: S,
    pub(crate) auth_orderer: AuthOrderer, // @TODO: This should probably be the state instead.
    pub(crate) key_manager_y: KeyManagerState,
    pub(crate) key_registry_y: KeyRegistryState,
    pub(crate) rng: Rng,
    _marker: PhantomData<(M, C, RS)>,
}

impl<S, F, M, C, RS> Manager<S, F, M, C, RS>
where
    S: SpacesStore,
    F: Forge<M>,
    C: Conditions,
    RS: Debug + Resolver<ActorId, OperationId, C, AuthOrderer, AuthDummyStore>,
{
    pub fn new(
        store: S,
        forge: F,
        identity_secret: &SecretKey,
        rng: Rng,
    ) -> Result<Self, ManagerError<M, C, RS>> {
        let auth_orderer = AuthOrderer::new();

        let key_manager_y = KeyManager::init(identity_secret, Lifetime::default(), &rng)?;

        let key_registry_y = KeyRegistry::init();

        let inner = ManagerInner {
            forge,
            store,
            auth_orderer,
            key_manager_y,
            key_registry_y,
            rng,
            _marker: PhantomData,
        };

        Ok(Self {
            inner: Rc::new(RefCell::new(inner)),
        })
    }

    pub fn space(&self) -> Space<S, F, M, C, RS> {
        todo!()
    }

    pub fn create_space(&self) -> Result<Space<S, F, M, C, RS>, ManagerError<M, C, RS>> {
        let space = Space::create(self.clone(), Vec::new())?;
        Ok(space)
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

#[derive(Debug, Error)]
pub enum ManagerError<M, C, RS>
where
    C: Conditions,
    RS: Resolver<ActorId, OperationId, C, AuthOrderer, AuthDummyStore>,
{
    #[error(transparent)]
    Space(#[from] SpaceError<M, C, RS>),

    #[error(transparent)]
    KeyManager(#[from] KeyManagerError),
}

#[cfg(test)]
mod tests {
    use std::convert::Infallible;

    use p2panda_core::PrivateKey;
    use p2panda_encryption::Rng;
    use p2panda_encryption::crypto::x25519::SecretKey;

    use crate::store::{AllState, MemoryStore};
    use crate::traits::Forge;
    use crate::{Conditions, StrongRemoveResolver};

    use super::Manager;

    #[derive(Debug)]
    struct Message {}

    #[derive(Debug)]
    struct TestForge {
        private_key: PrivateKey,
    }

    #[derive(Clone, Debug, PartialEq, PartialOrd)]
    struct TestConditions {}
    impl Conditions for TestConditions {}

    impl Forge<Message> for TestForge {
        type Error = Infallible;

        fn public_key(&self) -> p2panda_core::PublicKey {
            self.private_key.public_key()
        }

        fn forge(&self, args: crate::traits::ForgeArgs) -> Result<Message, Self::Error> {
            todo!()
        }

        fn forge_with(
            &self,
            private_key: p2panda_core::PrivateKey,
            args: crate::traits::ForgeArgs,
        ) -> Result<Message, Self::Error> {
            todo!()
        }
    }

    #[test]
    fn create_space() {
        let rng = Rng::from_seed([0; 32]);
        let private_key = PrivateKey::new();
        // @TODO: this should soon be a SQLite store.
        let mut store = MemoryStore::new(AllState::default());
        let forge = TestForge { private_key };
        let identity_secret = SecretKey::from_bytes(rng.random_array().unwrap());
        let manager: Manager<_, _, _, TestConditions, StrongRemoveResolver<TestConditions>> =
            Manager::new(store, forge, &identity_secret, rng).unwrap();
        let space = manager.create_space().unwrap();
        // println!("{0:#?}", space);
    }
}
