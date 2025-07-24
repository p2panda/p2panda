// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fmt::Debug;
use std::marker::PhantomData;
use std::sync::Arc;

use p2panda_auth::Access;
use p2panda_auth::group::GroupMember;
use p2panda_auth::traits::Resolver;
use p2panda_encryption::Rng;
use p2panda_encryption::key_manager::{KeyManager, KeyManagerError};
use p2panda_encryption::key_registry::KeyRegistry;
use p2panda_encryption::traits::PreKeyManager;
use thiserror::Error;
use tokio::sync::RwLock;

use crate::auth::orderer::AuthOrderer;
use crate::forge::Forge;
use crate::member::Member;
use crate::message::{AuthoredMessage, SpacesArgs, SpacesMessage};
use crate::space::{Space, SpaceError};
use crate::store::{KeyStore, SpaceStore};
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
    pub(crate) inner: Arc<RwLock<ManagerInner<S, F, M, C, RS>>>,
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
    S: SpaceStore<M, C, RS> + KeyStore,
    F: Forge<M, C>,
    M: AuthoredMessage + SpacesMessage<C>,
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
            inner: Arc::new(RwLock::new(inner)),
        })
    }

    pub fn space(&self, _space_id: ActorId) -> Space<S, F, M, C, RS> {
        todo!()
    }

    #[allow(clippy::type_complexity, clippy::result_large_err)]
    pub async fn create_space(
        &self,
        initial_members: &[(ActorId, Access<C>)],
    ) -> Result<(Space<S, F, M, C, RS>, M), ManagerError<S, F, M, C, RS>> {
        // @TODO: Check if initial members are known and have a key bundle present, throw error
        // otherwise.

        // @TODO: Assign GroupMember type to every actor based on looking up our own state,
        // checking if actor is a group or individual.

        // @TODO: Throw error when user tries to add a space to a space.

        let initial_members = initial_members
            .iter()
            .map(|(actor, access)| (GroupMember::Individual(actor.to_owned()), access.to_owned()))
            .collect();

        let (space, message) = Space::create(self.clone(), initial_members)
            .await
            .map_err(ManagerError::Space)?;

        Ok((space, message))
    }

    pub async fn id(&self) -> ActorId {
        let inner = self.inner.read().await;
        inner.forge.public_key().into()
    }

    pub async fn me(&self) -> Result<Member, ManagerError<S, F, M, C, RS>> {
        let inner = self.inner.read().await;

        let y = inner
            .store
            .key_manager()
            .await
            .map_err(ManagerError::KeyStore)?;

        // @TODO: What happens if the forge changes their private key?
        let my_id = inner.forge.public_key().into();

        Ok(Member::new(my_id, KeyManager::prekey_bundle(&y)))
    }

    pub async fn register_member(
        &mut self,
        member: &Member,
    ) -> Result<(), ManagerError<S, F, M, C, RS>> {
        // @TODO: Reject invalid / expired key bundles.

        let mut inner = self.inner.write().await;

        let y = inner
            .store
            .key_registry()
            .await
            .map_err(ManagerError::KeyStore)?;

        // @TODO: Setting longterm bundle should overwrite previous one if this is newer.
        let y_ii = KeyRegistry::add_longterm_bundle(y, member.id(), member.key_bundle().clone());

        inner
            .store
            .set_key_registry(&y_ii)
            .await
            .map_err(ManagerError::KeyStore)?;

        Ok(())
    }

    // We expect messages to be signature-checked, dependency-checked & partially ordered here.
    pub async fn process(&mut self, message: &M) -> Result<(), ManagerError<S, F, M, C, RS>> {
        // Route message to the regarding member-, group- or space processor.
        match message.args() {
            // Received key bundle from a member.
            SpacesArgs::KeyBundle {} => {
                // @TODO:
                // - Check if it is valid
                // - Store it in key manager if it is newer than our previously stored one (if given)
                todo!()
            }
            // Received control message related to a group or space.
            SpacesArgs::ControlMessage {
                id,
                control_message,
                ..
            } => {
                // @TODO:
                // - Detect if id is related to a space or group.
                // - Also process group messages.

                let has_space = {
                    let inner = self.inner.read().await;
                    inner
                        .store
                        .has_space(id)
                        .await
                        .map_err(ManagerError::SpaceStore)?
                };

                if !has_space && !control_message.is_create() {
                    // If this is not a "create" message we should have learned about the space
                    // before. This can be either a faulty message or a problem with the message
                    // orderer.
                    return Err(ManagerError::UnexpectedMessage(message.id()));
                }

                let mut space = Space::new(self.clone(), *id);
                space.process(message).await.map_err(ManagerError::Space)?;
            }
            // Received encrypted application data for a space.
            SpacesArgs::Application { space_id, .. } => {
                let has_space = {
                    let inner = self.inner.read().await;
                    inner
                        .store
                        .has_space(space_id)
                        .await
                        .map_err(ManagerError::SpaceStore)?
                };

                if !has_space {
                    return Err(ManagerError::UnexpectedMessage(message.id()));
                }

                let mut space = Space::new(self.clone(), *space_id);
                space.process(message).await.map_err(ManagerError::Space)?;
            }
        }

        // @TODO: Return events.

        Ok(())
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
    S: SpaceStore<M, C, RS> + KeyStore,
    F: Forge<M, C>,
    C: Conditions,
    RS: Resolver<ActorId, OperationId, C, AuthOrderer, AuthDummyStore>,
{
    #[error(transparent)]
    Space(#[from] SpaceError<S, F, M, C, RS>),

    #[error(transparent)]
    KeyManager(#[from] KeyManagerError),

    #[error("{0}")]
    KeyStore(<S as KeyStore>::Error),

    #[error("{0}")]
    SpaceStore(<S as SpaceStore<M, C, RS>>::Error),

    #[error("received unexpected message with id {0}, maybe it arrived out-of-order")]
    UnexpectedMessage(OperationId),
}

#[cfg(test)]
mod tests {
    use std::convert::Infallible;

    use p2panda_auth::Access;
    use p2panda_auth::group::GroupMember;
    use p2panda_core::{Hash, PrivateKey, PublicKey};
    use p2panda_encryption::Rng;
    use p2panda_encryption::crypto::x25519::SecretKey;
    use p2panda_encryption::key_bundle::Lifetime;
    use p2panda_encryption::key_manager::KeyManager;

    use crate::forge::Forge;
    use crate::message::{AuthoredMessage, ControlMessage, SpacesArgs, SpacesMessage};
    use crate::test_utils::MemoryStore;
    use crate::types::{ActorId, Conditions, OperationId, StrongRemoveResolver};

    use super::Manager;

    type SeqNum = u64;

    #[derive(Clone, Debug)]
    struct TestMessage {
        seq_num: SeqNum,
        public_key: PublicKey,
        spaces_args: SpacesArgs<TestConditions>,
    }

    impl AuthoredMessage for TestMessage {
        fn id(&self) -> OperationId {
            let mut buffer: Vec<u8> = self.public_key.as_bytes().to_vec();
            buffer.extend_from_slice(&self.seq_num.to_be_bytes());
            Hash::new(buffer).into()
        }

        fn author(&self) -> ActorId {
            self.public_key.into()
        }
    }

    impl SpacesMessage<TestConditions> for TestMessage {
        fn args(&self) -> &SpacesArgs<TestConditions> {
            &self.spaces_args
        }
    }

    #[derive(Debug)]
    struct TestForge {
        next_seq_num: SeqNum,
        private_key: PrivateKey,
    }

    impl TestForge {
        pub fn new(private_key: PrivateKey) -> Self {
            Self {
                next_seq_num: 0,
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

        async fn forge(
            &mut self,
            args: SpacesArgs<TestConditions>,
        ) -> Result<TestMessage, Self::Error> {
            let seq_num = self.next_seq_num;
            self.next_seq_num += 1;
            Ok(TestMessage {
                seq_num,
                public_key: self.public_key(),
                spaces_args: args,
            })
        }

        async fn forge_ephemeral(
            &mut self,
            private_key: PrivateKey,
            args: SpacesArgs<TestConditions>,
        ) -> Result<TestMessage, Self::Error> {
            Ok(TestMessage {
                // Will always be first entry in the "log" as we're dropping the private key.
                seq_num: 0,
                public_key: private_key.public_key(),
                spaces_args: args,
            })
        }
    }

    type TestStore = MemoryStore<TestMessage, TestConditions, StrongRemoveResolver<TestConditions>>;

    type TestManager = Manager<
        TestStore,
        TestForge,
        TestMessage,
        TestConditions,
        StrongRemoveResolver<TestConditions>,
    >;

    #[tokio::test]
    async fn create_space() {
        let rng = Rng::from_seed([0; 32]);

        let private_key = PrivateKey::new();
        let my_id: ActorId = private_key.public_key().into();

        // @TODO: We need a way to initialise our identity key when it is not set yet.
        let key_manager_y = {
            let identity_secret = SecretKey::from_bytes(rng.random_array().unwrap());
            KeyManager::init(&identity_secret, Lifetime::default(), &rng).unwrap()
        };

        let store = TestStore::new(my_id, key_manager_y);
        let forge = TestForge::new(private_key);

        let manager = TestManager::new(store, forge, rng).unwrap();

        // Methods return the correct identity handle.
        assert_eq!(manager.id().await, my_id);

        assert_eq!(manager.me().await.unwrap().id(), my_id);
        assert!(manager.me().await.unwrap().verify().is_ok());

        // Create Space
        // ~~~~~~~~~~~~

        let (mut space, message) = manager.create_space(&[]).await.unwrap();

        // We've added ourselves automatically with manage access.
        assert_eq!(
            space.members().await.unwrap(),
            vec![(my_id, Access::manage())]
        );

        let SpacesArgs::ControlMessage {
            id: group_id,
            control_message,
            direct_messages,
        } = message.args()
        else {
            panic!("expected system message");
        };

        assert_eq!(*group_id, space.id());

        // Control message contains "create".
        assert_eq!(
            control_message,
            &ControlMessage::Create {
                initial_members: vec![(GroupMember::Individual(my_id), Access::manage())]
            },
        );

        // No direct messages as we are the only member.
        assert!(direct_messages.is_empty());

        // Author of this message is _not_ us but an ephemeral key.
        assert_ne!(ActorId::from(message.public_key), manager.id().await);

        // Public key of this message is the space id.
        assert_eq!(ActorId::from(message.public_key), space.id());

        // Publish data
        // ~~~~~~~~~~~~

        let message = space.publish(b"Hello, Spaces!").await.unwrap();

        // Author of this message is us.
        assert_eq!(ActorId::from(message.public_key), manager.id().await);

        println!("{message:?}");
    }
}
