// SPDX-License-Identifier: MIT OR Apache-2.0

use std::convert::Infallible;
use std::hash::Hash as StdHash;
use std::marker::PhantomData;

use p2panda_auth::Access;
use p2panda_auth::group::GroupMember;
use p2panda_auth::traits::Conditions;
use p2panda_core::{Hash, PrivateKey, PublicKey};
use p2panda_encryption::Rng;
use p2panda_encryption::crypto::x25519::SecretKey;
use p2panda_encryption::data_scheme::DirectMessage;
use p2panda_encryption::key_bundle::Lifetime;
use p2panda_encryption::key_manager::KeyManager;

use crate::auth::orderer::AuthOrderer;
use crate::event::Event;
use crate::forge::Forge;
use crate::manager::Manager;
use crate::message::{AuthoredMessage, SpacesArgs, SpacesMessage};
use crate::space::SpaceError;
use crate::store::{AuthStore, SpaceStore};
use crate::test_utils::MemoryStore;
use crate::traits::SpaceId;
use crate::types::{
    ActorId, AuthControlMessage, AuthGroupAction, AuthGroupState, OperationId, StrongRemoveResolver,
};

type SeqNum = u64;

// Implement SpaceId for i32 which is what we use space identifiers in the tests.
impl SpaceId for i32 {}

#[derive(Clone, Debug)]
struct TestMessage<ID> {
    seq_num: SeqNum,
    public_key: PublicKey,
    spaces_args: SpacesArgs<ID, TestConditions>,
}

impl<ID> AuthoredMessage for TestMessage<ID>
where
    ID: SpaceId,
{
    fn id(&self) -> OperationId {
        let mut buffer: Vec<u8> = self.public_key.as_bytes().to_vec();
        buffer.extend_from_slice(&self.seq_num.to_be_bytes());
        Hash::new(buffer).into()
    }

    fn author(&self) -> ActorId {
        self.public_key.into()
    }
}

impl<ID> SpacesMessage<ID, TestConditions> for TestMessage<ID> {
    fn args(&self) -> &SpacesArgs<ID, TestConditions> {
        &self.spaces_args
    }
}

#[derive(Debug)]
struct TestForge<ID> {
    next_seq_num: SeqNum,
    private_key: PrivateKey,
    _phantom: PhantomData<ID>,
}

impl<ID> TestForge<ID> {
    pub fn new(private_key: PrivateKey) -> Self {
        Self {
            next_seq_num: 0,
            private_key,
            _phantom: PhantomData,
        }
    }
}

#[derive(Clone, Debug, PartialEq, PartialOrd)]
struct TestConditions {}

impl Conditions for TestConditions {}

impl<ID> Forge<ID, TestMessage<ID>, TestConditions> for TestForge<ID>
where
    ID: SpaceId,
{
    type Error = Infallible;

    fn public_key(&self) -> PublicKey {
        self.private_key.public_key()
    }

    async fn forge(
        &mut self,
        args: SpacesArgs<ID, TestConditions>,
    ) -> Result<TestMessage<ID>, Self::Error> {
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
        args: SpacesArgs<ID, TestConditions>,
    ) -> Result<TestMessage<ID>, Self::Error> {
        Ok(TestMessage {
            // Will always be first entry in the "log" as we're dropping the private key.
            seq_num: 0,
            public_key: private_key.public_key(),
            spaces_args: args,
        })
    }
}

type TestStore<ID> = MemoryStore<ID, TestMessage<ID>, TestConditions>;

type TestManager<ID> = Manager<
    ID,
    TestStore<ID>,
    TestForge<ID>,
    TestMessage<ID>,
    TestConditions,
    StrongRemoveResolver<TestConditions>,
>;

type TestSpaceError<ID> = SpaceError<
    ID,
    TestStore<ID>,
    TestForge<ID>,
    TestMessage<ID>,
    TestConditions,
    StrongRemoveResolver<TestConditions>,
>;

struct TestPeer<ID = i32> {
    id: u8,
    manager: TestManager<ID>,
}

impl<ID> TestPeer<ID>
where
    ID: SpaceId + StdHash,
{
    pub fn new(peer_id: u8) -> Self {
        let rng = Rng::from_seed([peer_id; 32]);

        let private_key = PrivateKey::from_bytes(&rng.random_array().unwrap());
        let my_id: ActorId = private_key.public_key().into();

        let key_manager_y = {
            let identity_secret = SecretKey::from_bytes(rng.random_array().unwrap());
            KeyManager::init(&identity_secret, Lifetime::default(), &rng).unwrap()
        };

        let orderer_y = AuthOrderer::init();
        let auth_y = AuthGroupState::new(orderer_y);
        let store = TestStore::new(my_id, key_manager_y, auth_y);
        let forge = TestForge::new(private_key);

        let manager = TestManager::new(store, forge, rng).unwrap();

        Self {
            id: peer_id,
            manager,
        }
    }
}

#[tokio::test]
async fn create_space() {
    let rng = Rng::from_seed([0; 32]);

    let private_key = PrivateKey::from_bytes(&rng.random_array().unwrap());
    let my_id: ActorId = private_key.public_key().into();

    // @TODO: We need a way to initialise our identity key when it is not set yet.
    let key_manager_y = {
        let identity_secret = SecretKey::from_bytes(rng.random_array().unwrap());
        KeyManager::init(&identity_secret, Lifetime::default(), &rng).unwrap()
    };

    let orderer_y = AuthOrderer::init();
    let auth_y = AuthGroupState::new(orderer_y);
    let store = TestStore::new(my_id, key_manager_y, auth_y);
    let forge = TestForge::new(private_key);

    let manager = TestManager::new(store, forge, rng).unwrap();

    // Methods return the correct identity handle.
    assert_eq!(manager.id().await, my_id);

    assert_eq!(manager.me().await.unwrap().id(), my_id);
    assert!(manager.me().await.unwrap().verify().is_ok());

    // Create Space
    // ~~~~~~~~~~~~

    let space_id = 0;
    let (space, messages) = manager.create_space(space_id, &[]).await.unwrap();

    // We've added ourselves automatically with manage access.
    assert_eq!(
        space.members().await.unwrap(),
        vec![(my_id, Access::manage())]
    );

    // There are two messages (one auth, one space)
    assert_eq!(messages.len(), 2);
    let message_01 = messages[0].clone();
    let message_02 = messages[1].clone();

    let SpacesArgs::Auth {
        control_message: auth_control_message,
        auth_dependencies,
    } = message_01.args()
    else {
        panic!("expected auth message");
    };

    let SpacesArgs::SpaceMembership {
        space_id,
        group_id,
        space_dependencies,
        auth_message_id,
        direct_messages,
    } = message_02.args()
    else {
        panic!("expected system message");
    };

    assert_eq!(*space_id, space.id());
    assert_eq!(*auth_message_id, message_01.id());

    // Dependencies are empty for both auth and encryption.
    assert_eq!(auth_dependencies, &vec![]);
    assert_eq!(space_dependencies.to_owned(), vec![]);

    // Control message contains "create".
    assert_eq!(
        auth_control_message.to_owned(),
        AuthControlMessage {
            group_id: *group_id,
            action: AuthGroupAction::Create {
                initial_members: vec![(GroupMember::Individual(my_id), Access::manage())]
            }
        },
    );

    // No direct messages as we are the only member.
    assert!(direct_messages.is_empty());

    // Orderer states have been updated.
    let manager_ref = manager.inner.read().await;
    let auth_y = manager_ref.store.auth().await.unwrap();
    assert_eq!(vec![message_01.id()], auth_y.orderer_y.heads());

    let y = manager_ref.store.space(&space.id()).await.unwrap().unwrap();
    assert_eq!(vec![message_02.id()], y.encryption_y.orderer.heads());

    // @TODO: Currently the "create" message has been signed by the author's permament key. We
    // would like to sign it with the ephemeral key instead.
    //
    // Author of this message is _not_ us but an ephemeral key.
    // assert_ne!(ActorId::from(message.public_key), manager.id().await);
    //
    // Public key of this message is the space id.
    // assert_eq!(ActorId::from(message.public_key), space.id());
}

#[tokio::test]
async fn send_and_receive() {
    let alice = TestPeer::new(0);
    let bob = TestPeer::new(1);

    // Manually register key bundles of all members.

    alice
        .manager
        .register_member(&bob.manager.me().await.unwrap())
        .await
        .unwrap();

    bob.manager
        .register_member(&alice.manager.me().await.unwrap())
        .await
        .unwrap();

    // Alice creates a space with Bob.

    let space_id = 0;
    let (alice_space, alice_messages) = alice
        .manager
        .create_space(space_id, &[(bob.manager.id().await, Access::write())])
        .await
        .unwrap();

    // @TODO: Currently the "create" message has been signed by the author's permament key. We
    // would like to sign it with the ephemeral key instead.
    // let alice_create_message = alice_create_messages.pop().unwrap();
    // assert_eq!(alice_create_message.author(), alice.manager.id().await);

    // Bob processes Alice's messages.

    for message in alice_messages {
        bob.manager.process(&message).await.unwrap();
    }

    // Bob sends a message to Alice.

    let bob_space = bob.manager.space(space_id).await.unwrap().unwrap();
    let message = bob_space.publish(b"Hello, Alice!").await.unwrap();

    // Bob's orderer state is updated.

    let manager_ref = bob.manager.inner.read().await;
    let bob_space_y = manager_ref.store.space(&space_id).await.unwrap().unwrap();
    assert_eq!(vec![message.id()], bob_space_y.encryption_y.orderer.heads());

    // Alice processes Bob's encrypted message.

    let events = alice.manager.process(&message).await.unwrap();
    assert_eq!(events.len(), 1);

    // Alice's orderer state is updated.

    let manager_ref = alice.manager.inner.read().await;
    let alice_space_y = manager_ref.store.space(&space_id).await.unwrap().unwrap();
    assert_eq!(
        vec![message.id()],
        alice_space_y.encryption_y.orderer.heads()
    );

    #[allow(irrefutable_let_patterns)]
    let Event::Application { space_id, data } = events.first().unwrap() else {
        panic!("unexpected event returned");
    };

    assert_eq!(space_id, &alice_space.id());
    assert_eq!(data, b"Hello, Alice!");
}

#[tokio::test]
async fn add_member_to_space() {
    let alice = TestPeer::new(0);
    let bob = <TestPeer>::new(1);

    // Manually register bobs key bundle.

    alice
        .manager
        .register_member(&bob.manager.me().await.unwrap())
        .await
        .unwrap();

    let alice_id = alice.manager.id().await;
    let bob_id = bob.manager.id().await;

    let manager = alice.manager.clone();

    // Create Space
    // ~~~~~~~~~~~~

    let space_id = 0;
    let (space, messages) = manager.create_space(space_id, &[]).await.unwrap();

    // There are two messages (one auth, and one space)
    assert_eq!(messages.len(), 2);
    let message_01 = messages[0].clone();
    let message_02 = messages[1].clone();

    drop(space);

    // Add new member to Space
    // ~~~~~~~~~~~~

    let space = manager.space(space_id).await.unwrap().unwrap();
    let messages = space
        .add(bob.manager.id().await, Access::read())
        .await
        .unwrap();
    let mut members = space.members().await.unwrap();
    drop(space);

    // There are two messages (one auth, and one space)
    assert_eq!(messages.len(), 2);
    let message_03 = messages[0].clone();
    let message_04 = messages[1].clone();

    let SpacesArgs::Auth {
        control_message: auth_control_message,
        auth_dependencies,
    } = message_03.args()
    else {
        panic!("expected auth message");
    };

    let SpacesArgs::SpaceMembership {
        space_id,
        group_id,
        space_dependencies,
        direct_messages,
        ..
    } = message_04.args()
    else {
        panic!("expected system message");
    };

    // Alice and bob are both members.
    members.sort_by(|(actor_a, _), (actor_b, _)| actor_a.cmp(actor_b));
    assert_eq!(
        members,
        vec![(alice_id, Access::manage()), (bob_id, Access::read())]
    );

    // Dependencies are set for both auth and encryption.
    assert_eq!(auth_dependencies.to_owned(), vec![message_01.id()]);
    assert_eq!(space_dependencies.to_owned(), vec![message_02.id()]);

    // Auth control message contains "add" for bob.
    assert_eq!(
        auth_control_message.to_owned(),
        AuthControlMessage {
            group_id: *group_id,
            action: AuthGroupAction::Add {
                member: GroupMember::Individual(bob_id),
                access: Access::read()
            }
        },
    );

    // There is one direct message and it's for bob.
    assert_eq!(direct_messages.len(), 1);
    let message = direct_messages.to_owned().pop().unwrap();
    assert!(matches!(
        message,
        DirectMessage {
            recipient,
            ..
        } if recipient == bob_id
    ));

    // Orderer states have been updated.
    let manager_ref = manager.inner.read().await;
    let y = manager_ref.store.space(&space_id).await.unwrap().unwrap();
    assert_eq!(vec![message_04.id()], y.encryption_y.orderer.heads());

    let auth_y = manager_ref.store.auth().await.unwrap();
    assert_eq!(vec![message_03.id()], auth_y.orderer_y.heads);
}

#[tokio::test]
async fn register_key_bundles_after_space_creation() {
    let alice = TestPeer::new(0);
    let bob = <TestPeer>::new(1);

    let manager = alice.manager.clone();

    // Create Space
    // ~~~~~~~~~~~~

    let space_id = 0;
    let (space, _) = manager.create_space(space_id, &[]).await.unwrap();
    drop(space);

    // Register key bundles _after_ the space was already created
    // ~~~~~~~~~~~~

    alice
        .manager
        .register_member(&bob.manager.me().await.unwrap())
        .await
        .unwrap();

    // Add new member to Space
    // ~~~~~~~~~~~~
    let space = manager.space(space_id).await.unwrap().unwrap();
    let result = space.add(bob.manager.id().await, Access::read()).await;

    assert!(result.is_ok());
}

#[tokio::test]
async fn send_and_receive_after_add() {
    let alice = TestPeer::new(0);
    let bob = TestPeer::new(1);

    let bob_id = bob.manager.id().await;

    // Manually register key bundles of all members.

    alice
        .manager
        .register_member(&bob.manager.me().await.unwrap())
        .await
        .unwrap();

    bob.manager
        .register_member(&alice.manager.me().await.unwrap())
        .await
        .unwrap();

    // Alice creates a space, adds Bob in a following step and then sends a message.

    let space_id = 0;
    let (alice_space, messages) = alice.manager.create_space(space_id, &[]).await.unwrap();
    let message_01 = messages[0].clone();
    let message_02 = messages[1].clone();
    let messages = alice_space.add(bob_id, Access::read()).await.unwrap();
    let message_03 = messages[0].clone();
    let message_04 = messages[1].clone();
    let message_05 = alice_space.publish(b"Hello bob").await.unwrap();

    // Bob processes all of Alice's messages.

    bob.manager.process(&message_01).await.unwrap();
    bob.manager.process(&message_02).await.unwrap();
    bob.manager.process(&message_03).await.unwrap();
    bob.manager.process(&message_04).await.unwrap();
    let events = bob.manager.process(&message_05).await.unwrap();
    assert_eq!(events.len(), 1);
}

#[tokio::test]
async fn add_pull_member_to_space() {
    let alice = TestPeer::new(0);
    let bob = <TestPeer>::new(1);

    // Manually register bobs key bundle.

    alice
        .manager
        .register_member(&bob.manager.me().await.unwrap())
        .await
        .unwrap();

    let alice_id = alice.manager.id().await;
    let bob_id = bob.manager.id().await;

    let manager = alice.manager.clone();

    // Create Space
    // ~~~~~~~~~~~~

    let space_id = 0;
    let (space, messages) = manager.create_space(space_id, &[]).await.unwrap();
    assert_eq!(messages.len(), 2);
    let message_01 = messages[0].clone();
    let message_02 = messages[1].clone();
    drop(space);

    // Add new pull-only member to Space
    // ~~~~~~~~~~~~

    let space = manager.space(space_id).await.unwrap().unwrap();
    let messages = space
        .add(bob.manager.id().await, Access::pull())
        .await
        .unwrap();
    let mut members = space.members().await.unwrap();

    // There are two messages (one auth, one space)
    assert_eq!(messages.len(), 2);
    let message_03 = messages[0].clone();
    let message_04 = messages[1].clone();

    let SpacesArgs::Auth {
        control_message: auth_control_message,
        auth_dependencies,
    } = message_03.args()
    else {
        panic!("expected auth message");
    };

    let SpacesArgs::SpaceMembership {
        space_id,
        group_id,
        space_dependencies,
        auth_message_id,
        direct_messages,
    } = message_04.args()
    else {
        panic!("expected system message");
    };

    assert_eq!(*space_id, space.id());
    assert_eq!(*auth_message_id, message_03.id());

    // Alice and bob are both members.
    members.sort_by(|(actor_a, _), (actor_b, _)| actor_a.cmp(actor_b));
    assert_eq!(
        members,
        vec![(alice_id, Access::manage()), (bob_id, Access::pull())]
    );

    assert_eq!(auth_dependencies.to_owned(), vec![message_01.id()]);
    // There is no space dependencies.
    assert_eq!(space_dependencies.to_owned(), vec![message_02.id()]);

    // Auth control message contains "add" for bob.
    assert_eq!(
        auth_control_message.to_owned(),
        AuthControlMessage {
            group_id: *group_id,
            action: AuthGroupAction::Add {
                member: GroupMember::Individual(bob_id),
                access: Access::pull()
            }
        },
    );

    // There are no direct messages.
    assert!(direct_messages.is_empty());

    let manager_ref = manager.inner.read().await;
    // Auth order has been updated.
    let auth_y = manager_ref.store.auth().await.unwrap();
    assert_eq!(vec![message_03.id()], auth_y.orderer_y.heads);

    let y = manager_ref.store.space(&space_id).await.unwrap().unwrap();
    // Encryption order has been updated.
    assert_eq!(vec![message_04.id()], y.encryption_y.orderer.heads());
}

#[tokio::test]
async fn receive_control_messages() {
    let alice = TestPeer::new(0);
    let bob = TestPeer::new(1);

    // Manually register bob's key bundle.

    alice
        .manager
        .register_member(&bob.manager.me().await.unwrap())
        .await
        .unwrap();

    // Manually register alice's key bundle.

    bob.manager
        .register_member(&alice.manager.me().await.unwrap())
        .await
        .unwrap();

    let alice_id = alice.manager.id().await;
    let bob_id = bob.manager.id().await;

    let alice_manager = alice.manager.clone();
    let bob_manager = bob.manager.clone();

    // Alice: Create Space
    // ~~~~~~~~~~~~

    let space_id = 0;
    let (space, messages) = alice_manager.create_space(space_id, &[]).await.unwrap();
    let group_id = space.group_id().await.unwrap();
    drop(space);

    // Bob: Receive Message 01 & 02
    // ~~~~~~~~~~~~

    let message_01 = messages[0].clone();
    let message_02 = messages[1].clone();
    bob.manager.process(&message_01).await.unwrap();

    // Global auth state has been updated.
    {
        let manager_ref = bob_manager.inner.read().await;
        let auth_y = manager_ref.store.auth().await.unwrap();
        let members = auth_y.members(group_id);
        assert_eq!(members, vec![(alice_id, Access::manage())]);
        assert_eq!(vec![message_01.id()], auth_y.orderer_y.heads());
    }

    bob.manager.process(&message_02).await.unwrap();
    let space = bob_manager.space(space_id).await.unwrap().unwrap();

    // Alice is the only group member.
    let members = space.members().await.unwrap();
    assert_eq!(members, vec![(alice_id, Access::manage())]);

    // Bob cannot publish to space as he is not welcomed yet.
    let error = space.publish(&[0, 1, 2]).await.unwrap_err();
    assert!(matches!(error, TestSpaceError::NotWelcomed(_)));

    // Orderer state has been updated.
    let manager_ref = bob_manager.inner.read().await;
    let y = manager_ref.store.space(&space_id).await.unwrap().unwrap();
    assert_eq!(vec![message_02.id()], y.encryption_y.orderer.heads());

    drop(manager_ref);

    // Alice: Publishes a message into the space
    // ~~~~~~~~~~~~

    let space = alice_manager.space(space_id).await.unwrap().unwrap();
    let message_03 = space.publish(&[0, 1, 2]).await.unwrap();

    // Alice: Add new member to Space
    // ~~~~~~~~~~~~

    let messages = space
        .add(bob.manager.id().await, Access::read())
        .await
        .unwrap();
    let message_04 = messages[0].clone();
    let message_05 = messages[1].clone();

    drop(space);

    // Bob: Receive Message 03, 04 and 05
    // ~~~~~~~~~~~~

    let events = bob.manager.process(&message_03).await.unwrap();
    assert!(events.is_empty());
    let _ = bob.manager.process(&message_04).await.unwrap();
    assert!(events.is_empty());
    let events = bob.manager.process(&message_05).await.unwrap();
    // The application message arrives only after bob is welcomed.
    assert_eq!(events.len(), 1);

    let space = bob_manager.space(space_id).await.unwrap().unwrap();
    // Alice and bob are both members.
    let mut members = space.members().await.unwrap();
    members.sort_by(|(actor_a, _), (actor_b, _)| actor_a.cmp(actor_b));
    assert_eq!(
        members,
        vec![(alice_id, Access::manage()), (bob_id, Access::read())]
    );

    // Orderer states have been updated.
    let manager_ref = bob_manager.inner.read().await;

    let auth_y = manager_ref.store.auth().await.unwrap();
    assert_eq!(vec![message_04.id()], auth_y.orderer_y.heads);

    let y = manager_ref.store.space(&space_id).await.unwrap().unwrap();
    assert_eq!(vec![message_05.id()], y.encryption_y.orderer.heads());
}

#[tokio::test]
async fn remove_member() {
    let alice = TestPeer::new(0);
    let bob = TestPeer::new(1);

    // Manually register key bundles on alice.

    alice
        .manager
        .register_member(&bob.manager.me().await.unwrap())
        .await
        .unwrap();

    // Manually register key bundles on bob.

    bob.manager
        .register_member(&alice.manager.me().await.unwrap())
        .await
        .unwrap();

    let bob_id = bob.manager.id().await;

    let alice_manager = alice.manager.clone();
    let bob_manager = bob.manager.clone();

    // Alice: Create Space with themselves and bob
    // ~~~~~~~~~~~~

    let space_id = 0;
    let (space, messages) = alice_manager
        .create_space(space_id, &[(bob_id, Access::read())])
        .await
        .unwrap();
    drop(space);

    // There are two messages (one auth, and one space)
    assert_eq!(messages.len(), 2);
    let message_01 = messages[0].clone();
    let message_02 = messages[1].clone();

    // Bob: Receive Message 01 & 02
    // ~~~~~~~~~~~~

    bob_manager.process(&message_01).await.unwrap();
    bob_manager.process(&message_02).await.unwrap();

    // Alice: Removes bob
    // ~~~~~~~~~~~~

    let space = alice_manager.space(space_id).await.unwrap().unwrap();
    let messages = space.remove(bob_id).await.unwrap();

    // There are two messages (one auth, and one space)
    assert_eq!(messages.len(), 2);
    let message_03 = messages[0].clone();
    let message_04 = messages[1].clone();

    let SpacesArgs::Auth {
        control_message: auth_control_message,
        ..
    } = message_03.args()
    else {
        panic!("expected auth message");
    };

    let SpacesArgs::SpaceMembership {
        group_id,
        direct_messages,
        ..
    } = message_04.args()
    else {
        panic!("expected system message");
    };

    // Auth control message contains "remove".
    assert_eq!(
        auth_control_message.to_owned(),
        AuthControlMessage {
            group_id: *group_id,
            action: AuthGroupAction::Remove {
                member: GroupMember::Individual(bob_id)
            }
        },
    );

    // There are no direct messages (Bob shouldn't receive the new group secret).
    assert!(direct_messages.is_empty());

    // Bob: Receive Message 03 & 04
    // ~~~~~~~~~~~~

    let events = bob_manager.process(&message_03).await.unwrap();
    assert!(events.is_empty());
    let events = bob_manager.process(&message_04).await.unwrap();
    assert!(matches!(events[0], Event::Removed { .. }));
}

#[tokio::test]
async fn concurrent_removal_conflict() {
    let alice = TestPeer::new(0);
    let bob = TestPeer::new(1);
    let claire = <TestPeer>::new(2);
    let dave = <TestPeer>::new(3);

    // Manually register all key bundles on alice.

    alice
        .manager
        .register_member(&bob.manager.me().await.unwrap())
        .await
        .unwrap();

    alice
        .manager
        .register_member(&claire.manager.me().await.unwrap())
        .await
        .unwrap();

    alice
        .manager
        .register_member(&dave.manager.me().await.unwrap())
        .await
        .unwrap();

    // Manually register all key bundles on bob.

    bob.manager
        .register_member(&alice.manager.me().await.unwrap())
        .await
        .unwrap();

    bob.manager
        .register_member(&claire.manager.me().await.unwrap())
        .await
        .unwrap();

    bob.manager
        .register_member(&dave.manager.me().await.unwrap())
        .await
        .unwrap();

    let bob_id = bob.manager.id().await;
    let claire_id = claire.manager.id().await;
    let dave_id = dave.manager.id().await;

    let alice_manager = alice.manager.clone();
    let bob_manager = bob.manager.clone();

    // Alice: Create Space with themselves and bob
    // ~~~~~~~~~~~~

    let space_id = 0;
    let (space, messages) = alice_manager
        .create_space(space_id, &[(bob_id, Access::manage())])
        .await
        .unwrap();
    drop(space);

    // There are two messages (one auth, and one space)
    assert_eq!(messages.len(), 2);
    let message_01 = messages[0].clone();
    let message_02 = messages[1].clone();

    // Bob: Receive alice's messages
    // ~~~~~~~~~~~~

    bob_manager.process(&message_01).await.unwrap();
    bob_manager.process(&message_02).await.unwrap();

    // Alice: Removes bob (concurrently)
    // ~~~~~~~~~~~~

    let space = alice_manager.space(space_id).await.unwrap().unwrap();
    let _ = space.remove(bob_id).await.unwrap();
    drop(space);

    // Bob: Adds claire (concurrently)
    // ~~~~~~~~~~~~

    let space = bob_manager.space(space_id).await.unwrap().unwrap();
    let messages = space.add(claire_id, Access::read()).await.unwrap();
    drop(space);

    // There are two messages (one auth, and one space)
    assert_eq!(messages.len(), 2);
    let message_03 = messages[0].clone();
    let message_04 = messages[1].clone();

    // Alice: process bobs' message
    // ~~~~~~~~~~~~

    alice_manager.process(&message_03).await.unwrap();
    alice_manager.process(&message_04).await.unwrap();

    // Alice: Adds dave
    // ~~~~~~~~~~~~

    let space = alice_manager.space(space_id).await.unwrap().unwrap();
    let messages = space.add(dave_id, Access::read()).await.unwrap();

    assert_eq!(messages.len(), 2);
    let message_05 = messages[0].clone();
    let message_06 = messages[1].clone();

    let SpacesArgs::Auth {
        control_message: auth_control_message,
        ..
    } = message_05.args()
    else {
        panic!("expected auth message");
    };

    let SpacesArgs::SpaceMembership {
        group_id,
        direct_messages,
        ..
    } = message_06.args()
    else {
        panic!("expected system message");
    };

    // Auth control message contains "remove".
    assert_eq!(
        auth_control_message.to_owned(),
        AuthControlMessage {
            group_id: *group_id,
            action: AuthGroupAction::Add {
                member: GroupMember::Individual(dave_id),
                access: Access::read()
            }
        },
    );

    // There is one direct message and it's for dave.
    assert_eq!(direct_messages.len(), 1);
    let message = direct_messages.to_owned().pop().unwrap();
    assert!(matches!(
        message,
        DirectMessage {
            recipient,
            ..
        } if recipient == dave_id
    ))
}

#[tokio::test]
async fn space_from_existing_auth_state() {
    let alice = TestPeer::new(0);
    let bob = <TestPeer>::new(1);
    let claire = <TestPeer>::new(2);

    let alice_id = alice.manager.id().await;
    let bob_id = bob.manager.id().await;
    let claire_id = claire.manager.id().await;

    let alice_manager = alice.manager.clone();
    let bob_manager = bob.manager.clone();
    let claire_manager = claire.manager.clone();

    // Manually register all key bundles on alice.

    alice_manager
        .register_member(&bob_manager.me().await.unwrap())
        .await
        .unwrap();

    alice_manager
        .register_member(&claire_manager.me().await.unwrap())
        .await
        .unwrap();

    // Create Group with bob and claire as managers.
    // ~~~~~~~~~~~~

    let (group, messages) = alice_manager
        .create_group(&[(bob_id, Access::manage()), (claire_id, Access::manage())])
        .await
        .unwrap();
    let member_group_id = group.id();

    assert_eq!(messages.len(), 1);
    let message_01 = messages[0].clone();

    // Create Space with group as member
    // ~~~~~~~~~~~~

    let space_id = 0;
    let (space, messages) = alice_manager
        .create_space(space_id, &[(member_group_id, Access::read())])
        .await
        .unwrap();

    // There are 3 messages:
    // 1) auth message containing "create" for the space group
    // 2) space message containing reference to auth "create" message for the member group
    // 3) space message containing reference to auth "create" message for the space
    assert_eq!(messages.len(), 3);
    let message_02 = messages[0].clone();
    let message_03 = messages[1].clone();
    let message_04 = messages[2].clone();

    let SpacesArgs::Auth {
        control_message: auth_control_message,
        ..
    } = message_02.args()
    else {
        panic!("expected auth message");
    };

    // Auth control message contains "create" for the space group.
    assert_eq!(
        auth_control_message.to_owned(),
        AuthControlMessage {
            group_id: space.group_id().await.unwrap(),
            action: AuthGroupAction::Create {
                initial_members: vec![
                    (GroupMember::Group(member_group_id), Access::read()),
                    (GroupMember::Individual(alice_id), Access::manage()),
                ],
            }
        },
    );

    let SpacesArgs::SpaceMembership {
        direct_messages,
        auth_message_id,
        ..
    } = message_03.args()
    else {
        panic!("expected system message");
    };

    // Space message references auth "create" message for the member group.
    assert_eq!(*auth_message_id, message_01.id());

    // There are no encryption control message.
    assert!(direct_messages.is_empty());

    let SpacesArgs::SpaceMembership {
        direct_messages,
        auth_message_id,
        ..
    } = message_04.args()
    else {
        panic!("expected system message");
    };

    // Space message references auth "create" message for space group.
    assert_eq!(*auth_message_id, message_02.id());

    // There are two direct messages.
    assert_eq!(direct_messages.len(), 2);

    // The messages are for bob and claire.
    let result = direct_messages.iter().all(|message| {
        matches!(
            message,
            DirectMessage {
                recipient,
                ..
            } if recipient == &bob_id || recipient == &claire_id
        )
    });
    assert!(result, "{:?}", direct_messages);

    // Space members are correct.
    let mut members = space.members().await.unwrap();
    members.sort_by(|(actor_a, _), (actor_b, _)| actor_a.cmp(actor_b));
    assert_eq!(
        members,
        vec![
            (alice_id, Access::manage()),
            (bob_id, Access::read()),
            (claire_id, Access::read()),
        ]
    );
}

#[tokio::test]
async fn create_group() {
    let alice = <TestPeer>::new(0);
    let bob = <TestPeer>::new(1);

    let alice_id = alice.manager.id().await;
    let bob_id = bob.manager.id().await;
    let manager = alice.manager.clone();

    // Create Group
    // ~~~~~~~~~~~~

    let (group, messages) = manager
        .create_group(&[(alice_id, Access::manage()), (bob_id, Access::manage())])
        .await
        .unwrap();

    assert_eq!(messages.len(), 1);
    let message_01 = messages[0].clone();

    let mut members = group.members().await.unwrap();
    members.sort_by(|(actor_a, _), (actor_b, _)| actor_a.cmp(actor_b));
    assert_eq!(
        members,
        vec![(alice_id, Access::manage()), (bob_id, Access::manage())]
    );

    // There is one auth message.
    let SpacesArgs::Auth {
        control_message,
        auth_dependencies,
    } = message_01.args()
    else {
        panic!("expected auth message");
    };

    // Dependencies are empty.
    assert_eq!(auth_dependencies, &vec![]);

    // Control message contains "create".
    assert_eq!(
        control_message.to_owned(),
        AuthControlMessage {
            group_id: group.id(),
            action: AuthGroupAction::Create {
                initial_members: vec![
                    (GroupMember::Individual(alice_id), Access::manage()),
                    (GroupMember::Individual(bob_id), Access::manage())
                ]
            }
        },
    );

    // Orderer state has been updated.
    let manager_ref = manager.inner.read().await;
    let auth_y = manager_ref.store.auth().await.unwrap();
    assert_eq!(vec![message_01.id()], auth_y.orderer_y.heads());
}

#[tokio::test]
async fn add_member_to_group() {
    let alice = <TestPeer>::new(0);
    let bob = <TestPeer>::new(1);
    let claire = <TestPeer>::new(2);

    let alice_id = alice.manager.id().await;
    let bob_id = bob.manager.id().await;
    let claire_id = claire.manager.id().await;
    let manager = alice.manager.clone();

    // Create Group
    // ~~~~~~~~~~~~

    let (group, messages) = manager
        .create_group(&[(alice_id, Access::manage()), (bob_id, Access::manage())])
        .await
        .unwrap();

    assert_eq!(messages.len(), 1);
    let message_01 = messages[0].clone();

    let messages = group.add(claire_id, Access::read()).await.unwrap();
    assert_eq!(messages.len(), 1);
    let message_02 = messages[0].clone();

    let mut members = group.members().await.unwrap();
    members.sort_by(|(actor_a, _), (actor_b, _)| actor_a.cmp(actor_b));
    assert_eq!(
        members,
        vec![
            (alice_id, Access::manage()),
            (bob_id, Access::manage()),
            (claire_id, Access::read())
        ]
    );

    // There is one auth message.
    let SpacesArgs::Auth {
        control_message,
        auth_dependencies,
    } = message_02.args()
    else {
        panic!("expected auth message");
    };

    // Dependencies contain message_01.
    assert_eq!(auth_dependencies, &vec![message_01.id()]);

    // Control message contains "add" of claire.
    assert_eq!(
        control_message.to_owned(),
        AuthControlMessage {
            group_id: group.id(),
            action: AuthGroupAction::Add {
                member: GroupMember::Individual(claire_id),
                access: Access::read()
            }
        },
    );

    // Orderer state has been updated.
    let manager_ref = manager.inner.read().await;
    let auth_y = manager_ref.store.auth().await.unwrap();
    assert_eq!(vec![message_02.id()], auth_y.orderer_y.heads());
}

#[tokio::test]
async fn remove_member_from_group() {
    let alice = <TestPeer>::new(0);
    let bob = <TestPeer>::new(1);

    let alice_id = alice.manager.id().await;
    let bob_id = bob.manager.id().await;
    let manager = alice.manager.clone();

    // Create Group
    // ~~~~~~~~~~~~

    let (group, messages) = manager
        .create_group(&[(alice_id, Access::manage()), (bob_id, Access::manage())])
        .await
        .unwrap();
    assert_eq!(messages.len(), 1);
    let message_01 = messages[0].clone();

    // Remove bob from group
    // ~~~~~~~~~~~~

    let messages = group.remove(bob_id).await.unwrap();
    assert_eq!(messages.len(), 1);
    let message_02 = messages[0].clone();

    let members = group.members().await.unwrap();
    assert_eq!(members, vec![(alice_id, Access::manage()),]);

    // There is one auth message.
    let SpacesArgs::Auth {
        control_message,
        auth_dependencies,
    } = message_02.args()
    else {
        panic!("expected auth message");
    };

    // Dependencies contain message_01.
    assert_eq!(auth_dependencies, &vec![message_01.id()]);

    // Control message contains "remove" of bob.
    assert_eq!(
        control_message.to_owned(),
        AuthControlMessage {
            group_id: group.id(),
            action: AuthGroupAction::Remove {
                member: GroupMember::Individual(bob_id),
            }
        },
    );

    // Orderer state has been updated.
    let manager_ref = manager.inner.read().await;
    let auth_y = manager_ref.store.auth().await.unwrap();
    assert_eq!(vec![message_02.id()], auth_y.orderer_y.heads());
}

#[tokio::test]
async fn receive_auth_messages() {
    let alice = <TestPeer>::new(0);
    let bob = <TestPeer>::new(1);
    let claire = <TestPeer>::new(2);

    let alice_id = alice.manager.id().await;
    let bob_id = bob.manager.id().await;
    let claire_id = claire.manager.id().await;

    let alice_manager = alice.manager.clone();
    let bob_manager = bob.manager.clone();

    // Create Group
    // ~~~~~~~~~~~~

    let (group, messages) = alice_manager
        .create_group(&[(alice_id, Access::manage()), (bob_id, Access::manage())])
        .await
        .unwrap();
    let group_id = group.id();
    assert_eq!(messages.len(), 1);
    let message_01 = messages[0].clone();

    // Add claire
    // ~~~~~~~~~~~~

    let messages = group.add(claire_id, Access::read()).await.unwrap();
    assert_eq!(messages.len(), 1);
    let message_02 = messages[0].clone();
    drop(group);

    // Bob receives message 01 & 02
    // ~~~~~~~~~~~~

    let _events = bob_manager.process(&message_01).await.unwrap();
    let _events = bob_manager.process(&message_02).await.unwrap();

    let group = bob_manager.group(group_id).await.unwrap().unwrap();
    let mut members = group.members().await.unwrap();
    members.sort_by(|(actor_a, _), (actor_b, _)| actor_a.cmp(actor_b));
    assert_eq!(
        members,
        vec![
            (alice_id, Access::manage()),
            (bob_id, Access::manage()),
            (claire_id, Access::read())
        ]
    );
    drop(group);

    // Orderer state has been updated.
    let manager_ref = bob_manager.inner.read().await;
    let auth_y = manager_ref.store.auth().await.unwrap();
    assert_eq!(vec![message_02.id()], auth_y.orderer_y.heads());
}

#[tokio::test]
async fn shared_auth_state() {
    let alice = TestPeer::new(0);
    let bob = <TestPeer>::new(1);
    let claire = <TestPeer>::new(2);

    // Manually register bobs key bundle.

    alice
        .manager
        .register_member(&bob.manager.me().await.unwrap())
        .await
        .unwrap();

    alice
        .manager
        .register_member(&claire.manager.me().await.unwrap())
        .await
        .unwrap();

    let alice_id = alice.manager.id().await;
    let bob_id = bob.manager.id().await;
    let claire_id = claire.manager.id().await;

    let manager = alice.manager.clone();

    // Create Space 0
    // ~~~~~~~~~~~~

    let space_id = 0;
    let (space_0, messages) = manager
        .create_space(space_id, &[(alice_id, Access::manage())])
        .await
        .unwrap();

    assert_eq!(messages.len(), 2);

    // Create Space 1
    // ~~~~~~~~~~~~

    let space_id = 1;
    let (space_1, messages) = manager
        .create_space(space_id, &[(alice_id, Access::manage())])
        .await
        .unwrap();
    // There are four messages (one auth, and three space)
    assert_eq!(messages.len(), 4);

    // Create group
    // ~~~~~~~~~~~~

    let (group, messages) = manager
        .create_group(&[(alice_id, Access::manage()), (bob_id, Access::read())])
        .await
        .unwrap();

    // There are three messages (one auth, and two space)
    assert_eq!(messages.len(), 3);

    // Add group to space 0
    // ~~~~~~~~~~~~

    let messages = space_0.add(group.id(), Access::read()).await.unwrap();
    // There are three messages (one auth, and two space)
    assert_eq!(messages.len(), 3);

    // Add group to space 1
    // ~~~~~~~~~~~~

    let messages = space_1.add(group.id(), Access::read()).await.unwrap();
    // There are three messages (one auth, and two space)
    assert_eq!(messages.len(), 3);

    // Add claire to the group
    // ~~~~~~~~~~~~

    let messages = group.add(claire_id, Access::read()).await.unwrap();
    // There are three messages (one auth, and two space)
    assert_eq!(messages.len(), 3);

    // Both space 0 and space 1 should now include claire.
    let expected_members = vec![
        (alice_id, Access::manage()),
        (bob_id, Access::read()),
        (claire_id, Access::read()),
    ];

    let mut members = space_0.members().await.unwrap();
    members.sort_by(|(actor_a, _), (actor_b, _)| actor_a.cmp(actor_b));
    assert_eq!(expected_members, members);

    let mut members = space_1.members().await.unwrap();
    members.sort_by(|(actor_a, _), (actor_b, _)| actor_a.cmp(actor_b));
    assert_eq!(expected_members, members);
}
