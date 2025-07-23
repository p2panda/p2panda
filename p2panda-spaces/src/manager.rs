// SPDX-License-Identifier: MIT OR Apache-2.0

use std::cell::RefCell;
use std::fmt::Debug;
use std::marker::PhantomData;
use std::rc::Rc;

use p2panda_auth::Access;
use p2panda_auth::group::GroupMember;
use p2panda_auth::traits::Resolver;
use p2panda_encryption::Rng;
use p2panda_encryption::key_manager::KeyManagerError;
use thiserror::Error;

use crate::auth::orderer::AuthOrderer;
use crate::forge::{Forge, ForgedMessage};
use crate::space::{Space, SpaceError};
use crate::store::SpacesStore;
use crate::types::{ActorId, AuthDummyStore, Conditions, OperationId};

// Create and manage spaces and groups.
//
// Takes care of ingesting operations, updating spaces, groups and member key-material. Has access
// to the operation and group stores, orderer, key-registry and key-manager.
//
// Routes operations to the correct space(s), group(s) or member.
//
// Only one instance of `Spaces` per app user.
//
// Operations are created and published within the spaces service, reacting to arriving
// operations, due to api calls (create group, create space), or triggered by key-bundles
// expiring.
//
// Users of spaces can subscribe to events which inform about member, group or space state
// changes, application data being decrypted, pre-key bundles being published, we were added or
// removed from a space.
//
// Is agnostic to current p2panda-streams, networking layer, data type.
#[derive(Debug)]
pub struct Manager<S, F, M, C, RS> {
    #[allow(clippy::type_complexity)]
    pub(crate) inner: Rc<RefCell<ManagerInner<S, F, M, C, RS>>>,
}

#[derive(Debug)]
pub(crate) struct ManagerInner<S, F, M, C, RS> {
    pub(crate) store: S,
    pub(crate) forge: F,
    pub(crate) rng: Rng,
    _marker: PhantomData<(M, C, RS)>,
}

impl<S, F, M, C, RS> Manager<S, F, M, C, RS>
where
    S: SpacesStore,
    F: Forge<M, C>,
    M: ForgedMessage<C>,
    C: Conditions,
    // @TODO: Can we get rid of this Debug requirement here?
    RS: Debug + Resolver<ActorId, OperationId, C, AuthOrderer, AuthDummyStore>,
{
    #[allow(clippy::result_large_err)]
    pub fn new(store: S, forge: F, rng: Rng) -> Result<Self, ManagerError<S, F, M, C, RS>> {
        let inner = ManagerInner {
            store,
            forge,
            rng,
            _marker: PhantomData,
        };

        Ok(Self {
            inner: Rc::new(RefCell::new(inner)),
        })
    }

    pub fn space(&self, _space_id: ActorId) -> Space<S, F, M, C, RS> {
        todo!()
    }

    #[allow(clippy::type_complexity, clippy::result_large_err)]
    pub async fn create_space(
        &self,
        initial_members: &[(ActorId, Access<C>)],
    ) -> Result<Space<S, F, M, C, RS>, ManagerError<S, F, M, C, RS>> {
        // @TODO: Assign GroupMember type to every actor based on looking up our own state,
        // checking if actor is a group or individual.
        // @TODO: Throw error when user tries to add a space to a space.
        let initial_members = initial_members
            .iter()
            .map(|(actor, access)| (GroupMember::Individual(actor.to_owned()), access.to_owned()))
            .collect();

        let space = Space::create(self.clone(), initial_members)
            .await
            .map_err(ManagerError::Space)?;

        Ok(space)
    }

    pub fn register_member(&mut self) {
        // @TODO: Find a better name
        // @TODO: Implement manually adding an "individual" key bundle to the registry.
    }

    pub fn process(&mut self, _message: &M) {
        // @TODO: Look up if we know about the space id in the message M, route it to the right
        // instance and continue processing there.
        //
        // This can be a system message (control messages) or application message (to-be decrypted
        // in space)

        // @TODO: Error when we process an message on an unknown space. This should not happen at
        // this stage because we rely on an orderer before.

        todo!()
    }
}

// Deriving clone on Manager will enforce generics to also impl Clone even though we are wrapping
// them in an Arc. Related: https://stackoverflow.com/questions/72150623
impl<S, F, M, C, RS> Clone for Manager<S, F, M, C, RS> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

#[derive(Debug, Error)]
#[allow(clippy::large_enum_variant)]
pub enum ManagerError<S, F, M, C, RS>
where
    S: SpacesStore,
    F: Forge<M, C>,
    M: ForgedMessage<C>,
    C: Conditions,
    RS: Resolver<ActorId, OperationId, C, AuthOrderer, AuthDummyStore>,
{
    #[error(transparent)]
    Space(#[from] SpaceError<S, F, M, C, RS>),

    #[error(transparent)]
    KeyManager(#[from] KeyManagerError),
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::convert::Infallible;

    use p2panda_core::{Hash, PrivateKey, PublicKey};
    use p2panda_encryption::Rng;

    use crate::forge::{Forge, ForgeArgs, ForgedMessage};
    use crate::message::ControlMessage;
    use crate::types::{ActorId, Conditions, OperationId, StrongRemoveResolver};

    use super::Manager;

    type SeqNum = u64;

    #[derive(Debug)]
    struct TestMessage {
        seq_num: SeqNum,
        public_key: PublicKey,
        spaces_args: ForgeArgs<TestConditions>,
    }

    impl ForgedMessage<TestConditions> for TestMessage {
        fn id(&self) -> OperationId {
            let mut buffer: Vec<u8> = self.public_key.as_bytes().to_vec();
            buffer.extend_from_slice(&self.seq_num.to_be_bytes());
            Hash::new(buffer).into()
        }

        fn author(&self) -> ActorId {
            self.public_key.into()
        }

        fn group_id(&self) -> ActorId {
            self.spaces_args.group_id
        }

        fn control_message(&self) -> &ControlMessage<TestConditions> {
            &self.spaces_args.control_message
        }
    }

    #[derive(Debug)]
    struct TestForge {
        next_seq_num: Cell<SeqNum>,
        private_key: PrivateKey,
    }

    impl TestForge {
        pub fn new(private_key: PrivateKey) -> Self {
            Self {
                next_seq_num: Cell::new(0),
                private_key,
            }
        }
    }

    #[derive(Clone, Debug, PartialEq, PartialOrd)]
    struct TestConditions {}

    impl Conditions for TestConditions {}

    impl Forge<TestMessage, TestConditions> for TestForge {
        type Error = Infallible;

        fn public_key(&self) -> PublicKey {
            self.private_key.public_key()
        }

        fn forge(&self, args: ForgeArgs<TestConditions>) -> Result<TestMessage, Self::Error> {
            Ok(TestMessage {
                seq_num: self.next_seq_num.replace(self.next_seq_num.get() + 1),
                public_key: self.public_key(),
                spaces_args: args,
            })
        }

        fn forge_ephemeral(
            &self,
            private_key: PrivateKey,
            args: ForgeArgs<TestConditions>,
        ) -> Result<TestMessage, Self::Error> {
            Ok(TestMessage {
                // Will always be first entry in the "log" as we're dropping the private key.
                seq_num: 0,
                public_key: private_key.public_key(),
                spaces_args: args,
            })
        }
    }

    #[test]
    fn create_space() {
        // let rng = Rng::from_seed([0; 32]);
        // let private_key = PrivateKey::new();

        // @TODO: this should soon be a SQLite store.
        // let store = MemoryStore::new(AllState::default());

        // let forge = TestForge::new(private_key);

        // let manager: Manager<_, _, _, TestConditions, StrongRemoveResolver<TestConditions>> =
        //     Manager::new(store, forge, rng).unwrap();
        //
        // let _space = manager.create_space(&[]).unwrap();
    }
}
