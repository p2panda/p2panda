// SPDX-License-Identifier: MIT OR Apache-2.0

use std::convert::Infallible;

use p2panda_auth::Access;
use p2panda_auth::group::GroupMember;
use p2panda_core::{Hash, PrivateKey, PublicKey};
use p2panda_encryption::Rng;
use p2panda_encryption::crypto::x25519::SecretKey;
use p2panda_encryption::key_bundle::Lifetime;
use p2panda_encryption::key_manager::KeyManager;

use crate::forge::Forge;
use crate::manager::Manager;
use crate::message::{AuthoredMessage, ControlMessage, SpacesArgs, SpacesMessage};
use crate::test_utils::MemoryStore;
use crate::types::{ActorId, Conditions, OperationId, StrongRemoveResolver};

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

        let store = TestStore::new(my_id, key_manager_y);
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

    let store = TestStore::new(my_id, key_manager_y);
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
    let mut alice = TestPeer::new(0);
    let mut bob = TestPeer::new(1);

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

    let mut bob_space = bob.manager.space(&alice_space.id()).await.unwrap().unwrap();

    let message = bob_space.publish(b"Hello, Alice!").await.unwrap();

    // Alice processes Bob's encrypted message.

    alice.manager.process(&message).await.unwrap();
}
