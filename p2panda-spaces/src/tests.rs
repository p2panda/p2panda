// SPDX-License-Identifier: MIT OR Apache-2.0

use std::convert::Infallible;

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
use crate::message::{AuthoredMessage, ControlMessage, SpacesArgs, SpacesMessage};
use crate::space::SpaceError;
use crate::store::{AuthStore, SpaceStore};
use crate::test_utils::MemoryStore;
use crate::types::{ActorId, AuthGroupState, OperationId, StrongRemoveResolver};

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

type TestStore = MemoryStore<TestMessage, TestConditions>;

type TestManager = Manager<
    TestStore,
    TestForge,
    TestMessage,
    TestConditions,
    StrongRemoveResolver<TestConditions>,
>;

type TestSpaceError = SpaceError<
    TestStore,
    TestForge,
    TestMessage,
    TestConditions,
    StrongRemoveResolver<TestConditions>,
>;

struct TestPeer {
    id: u8,
    manager: TestManager,
}

impl TestPeer {
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

    let (space, message) = manager.create_space(&[]).await.unwrap();

    // We've added ourselves automatically with manage access.
    assert_eq!(
        space.members().await.unwrap(),
        vec![(my_id, Access::manage())]
    );

    let SpacesArgs::ControlMessage {
        id: group_id,
        control_message,
        direct_messages,
        auth_dependencies,
        encryption_dependencies,
    } = message.args()
    else {
        panic!("expected system message");
    };

    assert_eq!(*group_id, space.id());

    // Dependencies are empty for both auth and encryption.
    assert_eq!(auth_dependencies.to_owned(), vec![]);
    assert_eq!(encryption_dependencies.to_owned(), vec![]);

    // Control message contains "create".
    assert_eq!(
        control_message,
        &ControlMessage::Create {
            initial_members: vec![(GroupMember::Individual(my_id), Access::manage())]
        },
    );

    // No direct messages as we are the only member.
    assert!(direct_messages.is_empty());

    // Orderer states have been updated.
    let manager_ref = manager.inner.read().await;
    let y = manager_ref.store.space(&space.id()).await.unwrap().unwrap();
    assert_eq!(vec![message.id()], y.encryption_y.orderer.heads());

    let auth_y = manager_ref.store.auth().await.unwrap();
    assert_eq!(vec![message.id()], auth_y.orderer_y.heads)

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

    let (alice_space, alice_create_message) = alice
        .manager
        .create_space(&[(bob.manager.id().await, Access::write())])
        .await
        .unwrap();

    // @TODO: Currently the "create" message has been signed by the author's permament key. We
    // would like to sign it with the ephemeral key instead.
    assert_eq!(alice_create_message.author(), alice.manager.id().await);

    // Bob processes Alice's "create" message.

    bob.manager.process(&alice_create_message).await.unwrap();

    // Bob sends a message to Alice.

    let bob_space = bob.manager.space(&alice_space.id()).await.unwrap().unwrap();

    let message = bob_space.publish(b"Hello, Alice!").await.unwrap();

    // Alice processes Bob's encrypted message.

    let events = alice.manager.process(&message).await.unwrap();
    assert_eq!(events.len(), 1);

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
    let bob = TestPeer::new(1);

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

    let (space, message_01) = manager.create_space(&[]).await.unwrap();
    let space_id = space.id();
    drop(space);

    // Orderer states have been updated.
    let manager_ref = manager.inner.read().await;
    let y = manager_ref.store.space(&space_id).await.unwrap().unwrap();
    assert_eq!(vec![message_01.id()], y.encryption_y.orderer.heads());

    let auth_y = manager_ref.store.auth().await.unwrap();
    assert_eq!(vec![message_01.id()], auth_y.orderer_y.heads);
    drop(manager_ref);

    // Add new member to Space
    // ~~~~~~~~~~~~

    let space = manager.space(&space_id).await.unwrap().unwrap();
    let message_02 = space
        .add(
            GroupMember::Individual(bob.manager.id().await),
            Access::read(),
        )
        .await
        .unwrap();
    let mut members = space.members().await.unwrap();
    drop(space);

    let SpacesArgs::ControlMessage {
        id: group_id,
        control_message,
        auth_dependencies,
        encryption_dependencies,
        direct_messages,
    } = message_02.args()
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
    assert_eq!(encryption_dependencies.to_owned(), vec![message_01.id()]);

    // Correct space id.
    assert_eq!(*group_id, space_id);

    // Control message contains "add".
    assert_eq!(
        control_message,
        &ControlMessage::Add {
            member: GroupMember::Individual(bob_id),
            access: Access::read()
        },
    );

    // Orderer states have been updated.
    let manager_ref = manager.inner.read().await;
    let y = manager_ref.store.space(&space_id).await.unwrap().unwrap();
    assert_eq!(vec![message_02.id()], y.encryption_y.orderer.heads());

    let auth_y = manager_ref.store.auth().await.unwrap();
    assert_eq!(vec![message_02.id()], auth_y.orderer_y.heads);

    // There is one direct message and it's for bob.
    assert_eq!(direct_messages.len(), 1);
    let message = direct_messages.to_owned().pop().unwrap();
    assert!(matches!(
        message,
        DirectMessage {
            recipient,
            ..
        } if recipient == bob_id
    ))
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

    let (alice_space, message_01) = alice.manager.create_space(&[]).await.unwrap();
    let message_02 = alice_space
        .add(GroupMember::Individual(bob_id), Access::read())
        .await
        .unwrap();
    let message_03 = alice_space.publish(b"Hello bob").await.unwrap();

    // Bob processes all of Alice's messages.

    bob.manager.process(&message_01).await.unwrap();
    bob.manager.process(&message_02).await.unwrap();
    let events = bob.manager.process(&message_03).await.unwrap();
    assert_eq!(events.len(), 1);
}

#[tokio::test]
async fn add_pull_member_to_space() {
    let alice = TestPeer::new(0);
    let bob = TestPeer::new(1);

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

    let (space, message_01) = manager.create_space(&[]).await.unwrap();
    let space_id = space.id();
    drop(space);

    // Add new pull-only member to Space
    // ~~~~~~~~~~~~

    let space = manager.space(&space_id).await.unwrap().unwrap();
    let message_02 = space
        .add(
            GroupMember::Individual(bob.manager.id().await),
            Access::pull(),
        )
        .await
        .unwrap();
    let mut members = space.members().await.unwrap();
    drop(space);

    let SpacesArgs::ControlMessage {
        id: group_id,
        control_message,
        auth_dependencies,
        encryption_dependencies,
        direct_messages,
    } = message_02.args()
    else {
        panic!("expected system message");
    };

    // Correct space id.
    assert_eq!(*group_id, space_id);

    // Alice and bob are both members.
    members.sort_by(|(actor_a, _), (actor_b, _)| actor_a.cmp(actor_b));
    assert_eq!(
        members,
        vec![(alice_id, Access::manage()), (bob_id, Access::pull())]
    );

    assert_eq!(auth_dependencies.to_owned(), vec![message_01.id()]);
    // There is no dependency for encryption.
    assert_eq!(encryption_dependencies.to_owned(), vec![]);

    // Control message contains "add".
    assert_eq!(
        control_message,
        &ControlMessage::Add {
            member: GroupMember::Individual(bob_id),
            access: Access::pull()
        },
    );

    let manager_ref = manager.inner.read().await;
    let y = manager_ref.store.space(&space_id).await.unwrap().unwrap();
    // Encryption order still has message_01 as it's latest state.
    assert_eq!(vec![message_01.id()], y.encryption_y.orderer.heads());

    // Auth order has been updated.
    let auth_y = manager_ref.store.auth().await.unwrap();
    assert_eq!(vec![message_02.id()], auth_y.orderer_y.heads);

    // There are no direct messages.
    assert_eq!(direct_messages.len(), 0);
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

    let (space, message_01) = alice_manager.create_space(&[]).await.unwrap();
    let space_id = space.id();
    drop(space);

    // Bob: Receive Message 01
    // ~~~~~~~~~~~~

    bob.manager.process(&message_01).await.unwrap();
    let space = bob_manager.space(&space_id).await.unwrap().unwrap();

    // Alice is the only group member.
    let members = space.members().await.unwrap();
    assert_eq!(members, vec![(alice_id, Access::manage())]);

    // Bob cannot publish to space as he is not welcomed yet.
    let error = space.publish(&[0, 1, 2]).await.unwrap_err();
    assert!(matches!(error, TestSpaceError::NotWelcomed(_)));

    // Orderer states have been updated.
    let manager_ref = bob_manager.inner.read().await;
    let y = manager_ref.store.space(&space_id).await.unwrap().unwrap();
    assert_eq!(vec![message_01.id()], y.encryption_y.orderer.heads());

    let auth_y = manager_ref.store.auth().await.unwrap();
    assert_eq!(vec![message_01.id()], auth_y.orderer_y.heads);
    drop(manager_ref);

    // Alice: Publishes a message into the space
    // ~~~~~~~~~~~~

    let space = alice_manager.space(&space_id).await.unwrap().unwrap();
    let message_02 = space.publish(&[0, 1, 2]).await.unwrap();

    // Alice: Add new member to Space
    // ~~~~~~~~~~~~

    let message_03 = space
        .add(
            GroupMember::Individual(bob.manager.id().await),
            Access::read(),
        )
        .await
        .unwrap();

    drop(space);

    // Bob: Receive Message 02 & 03
    // ~~~~~~~~~~~~

    let events = bob.manager.process(&message_02).await.unwrap();
    assert!(events.is_empty());
    let events = bob.manager.process(&message_03).await.unwrap();
    // The application message arrives only after bob is welcomed.
    assert_eq!(events.len(), 1);
    let space = bob_manager.space(&space_id).await.unwrap().unwrap();

    // Alice and bob are both members.
    let mut members = space.members().await.unwrap();
    members.sort_by(|(actor_a, _), (actor_b, _)| actor_a.cmp(actor_b));
    assert_eq!(
        members,
        vec![(alice_id, Access::manage()), (bob_id, Access::read())]
    );

    // Orderer states have been updated.
    let manager_ref = bob_manager.inner.read().await;
    let y = manager_ref.store.space(&space_id).await.unwrap().unwrap();
    assert_eq!(vec![message_03.id()], y.encryption_y.orderer.heads());

    let auth_y = manager_ref.store.auth().await.unwrap();
    assert_eq!(vec![message_03.id()], auth_y.orderer_y.heads);
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

    let (space, message_01) = alice_manager
        .create_space(&[(bob_id, Access::read())])
        .await
        .unwrap();
    let space_id = space.id();
    drop(space);

    // Bob: Receive Message 01
    // ~~~~~~~~~~~~

    bob_manager.process(&message_01).await.unwrap();

    // Alice: Removes bob
    // ~~~~~~~~~~~~

    let space = alice_manager.space(&space_id).await.unwrap().unwrap();
    let message_02 = space.remove(GroupMember::Individual(bob_id)).await.unwrap();

    let SpacesArgs::ControlMessage {
        direct_messages, ..
    } = message_02.args()
    else {
        panic!("expected system message");
    };

    // There are no direct messages (Bob shouldn't receive the new group secret).
    assert_eq!(direct_messages.len(), 0);

    // Bob: Receive Message 02
    // ~~~~~~~~~~~~

    let events = bob_manager.process(&message_02).await.unwrap();
    let event = events.first().unwrap();
    assert!(matches!(event, Event::Removed { .. }));
}

#[tokio::test]
async fn concurrent_removal_conflict() {
    let alice = TestPeer::new(0);
    let bob = TestPeer::new(1);
    let claire = TestPeer::new(2);
    let dave = TestPeer::new(3);

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

    let (space, message_01) = alice_manager
        .create_space(&[(bob_id, Access::manage())])
        .await
        .unwrap();
    let space_id = space.id();
    drop(space);

    // Bob: Receive alice's message
    // ~~~~~~~~~~~~

    bob_manager.process(&message_01).await.unwrap();

    // Alice: Removes bob
    // ~~~~~~~~~~~~

    let space = alice_manager.space(&space_id).await.unwrap().unwrap();
    let _ = space.remove(GroupMember::Individual(bob_id)).await.unwrap();

    drop(space);

    // Bob: Adds claire
    // ~~~~~~~~~~~~

    let space = bob_manager.space(&space_id).await.unwrap().unwrap();
    let message_02_b = space
        .add(GroupMember::Individual(claire_id), Access::read())
        .await
        .unwrap();

    drop(space);

    // Alice: process bobs' message
    // ~~~~~~~~~~~~

    alice_manager.process(&message_02_b).await.unwrap();

    // Alice: Adds dave
    // ~~~~~~~~~~~~

    let space = alice_manager.space(&space_id).await.unwrap().unwrap();
    let message_03 = space
        .add(GroupMember::Individual(dave_id), Access::read())
        .await
        .unwrap();

    let SpacesArgs::ControlMessage {
        direct_messages, ..
    } = message_03.args()
    else {
        panic!("expected system message");
    };

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

