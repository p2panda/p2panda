// SPDX-License-Identifier: MIT OR Apache-2.0

use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use assert_matches::assert_matches;
use p2panda_auth::Access;
use p2panda_auth::group::GroupMember;
use p2panda_encryption::Rng;
use p2panda_encryption::crypto::x25519::SecretKey;
use p2panda_encryption::data_scheme::DirectMessage;
use p2panda_encryption::key_bundle::{Lifetime, LongTermKeyBundle, PreKey};

use crate::ActorId;
use crate::event::{Event, GroupActor, GroupContext, GroupEvent, SpaceContext, SpaceEvent};
use crate::member::Member;
use crate::message::SpacesArgs;
use crate::test_utils::{TestConditions, TestPeer, TestSpaceError};
use crate::traits::{AuthStore, AuthoredMessage, SpacesMessage, SpacesStore};
use crate::types::{AuthControlMessage, AuthGroupAction};

fn sort_group_actors(members: &mut Vec<(GroupActor, Access<TestConditions>)>) {
    members.sort_by(|(actor_a, _), (actor_b, _)| actor_a.id().cmp(&actor_b.id()));
}

fn sort_members(members: &mut Vec<(ActorId, Access<TestConditions>)>) {
    members.sort_by(|(actor_a, _), (actor_b, _)| actor_a.cmp(&actor_b));
}

#[tokio::test]
async fn create_space() {
    let alice = TestPeer::new(0).await;
    let manager = alice.manager.clone();
    let alice_id = manager.id();

    // Methods return the correct identity handle.
    assert_eq!(manager.id(), alice_id);

    assert_eq!(manager.me().await.unwrap().id(), alice_id);
    assert!(manager.me().await.unwrap().verify().is_ok());

    // Create Space
    // ~~~~~~~~~~~~

    let space_id = 0;
    let (space, messages, events) = manager.create_space(space_id, &[]).await.unwrap();

    // We've added ourselves automatically with manage access.
    assert_eq!(
        space.members().await.unwrap(),
        vec![(alice_id, Access::manage())]
    );

    // There are two events
    assert_eq!(events.len(), 2);

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
                initial_members: vec![(GroupMember::Individual(alice_id), Access::manage())]
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
}

#[tokio::test]
async fn send_and_receive() {
    let alice = TestPeer::new(0).await;
    let bob = TestPeer::new(1).await;

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
    let (alice_space, alice_messages, _) = alice
        .manager
        .create_space(space_id, &[(bob.manager.id(), Access::write())])
        .await
        .unwrap();

    // Bob processes Alice's messages.

    for message in alice_messages {
        bob.manager.persist_message(&message).await.unwrap();
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

    alice.manager.persist_message(&message).await.unwrap();
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
    let alice = TestPeer::new(0).await;
    let bob = <TestPeer>::new(1).await;

    // Manually register bobs key bundle.

    alice
        .manager
        .register_member(&bob.manager.me().await.unwrap())
        .await
        .unwrap();

    let alice_id = alice.manager.id();
    let bob_id = bob.manager.id();

    let manager = alice.manager.clone();

    // Create Space
    // ~~~~~~~~~~~~

    let space_id = 0;
    let (space, messages, _) = manager.create_space(space_id, &[]).await.unwrap();

    // There are two messages (one auth, and one space)
    assert_eq!(messages.len(), 2);
    let message_01 = messages[0].clone();
    let message_02 = messages[1].clone();

    drop(space);

    // Add new member to Space
    // ~~~~~~~~~~~~

    let space = manager.space(space_id).await.unwrap().unwrap();
    let (messages, _) = space.add(bob.manager.id(), Access::read()).await.unwrap();
    let members = space.members().await.unwrap();
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
    let alice = TestPeer::new(0).await;
    let bob = <TestPeer>::new(1).await;

    let manager = alice.manager.clone();

    // Create Space
    // ~~~~~~~~~~~~

    let space_id = 0;
    let (space, _, _) = manager.create_space(space_id, &[]).await.unwrap();
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
    let result = space.add(bob.manager.id(), Access::read()).await;

    assert!(result.is_ok());
}

#[tokio::test]
async fn send_and_receive_after_add() {
    let alice = TestPeer::new(0).await;
    let bob = TestPeer::new(1).await;

    let bob_id = bob.manager.id();

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
    let (alice_space, messages, _) = alice.manager.create_space(space_id, &[]).await.unwrap();
    let message_01 = messages[0].clone();
    let message_02 = messages[1].clone();
    let (messages, _) = alice_space.add(bob_id, Access::read()).await.unwrap();
    let message_03 = messages[0].clone();
    let message_04 = messages[1].clone();
    let message_05 = alice_space.publish(b"Hello bob").await.unwrap();

    // Bob processes all of Alice's messages.

    bob.manager.persist_message(&message_01).await.unwrap();
    bob.manager.process(&message_01).await.unwrap();
    bob.manager.persist_message(&message_02).await.unwrap();
    bob.manager.process(&message_02).await.unwrap();
    bob.manager.persist_message(&message_03).await.unwrap();
    bob.manager.process(&message_03).await.unwrap();
    bob.manager.persist_message(&message_04).await.unwrap();
    bob.manager.process(&message_04).await.unwrap();
    bob.manager.persist_message(&message_05).await.unwrap();
    let events = bob.manager.process(&message_05).await.unwrap();
    assert_eq!(events.len(), 1);
}

#[tokio::test]
async fn add_pull_member_to_space() {
    let alice = TestPeer::new(0).await;
    let bob = <TestPeer>::new(1).await;

    // Manually register bobs key bundle.

    alice
        .manager
        .register_member(&bob.manager.me().await.unwrap())
        .await
        .unwrap();

    let alice_id = alice.manager.id();
    let bob_id = bob.manager.id();

    let manager = alice.manager.clone();

    // Create Space
    // ~~~~~~~~~~~~

    let space_id = 0;
    let (space, messages, _) = manager.create_space(space_id, &[]).await.unwrap();
    assert_eq!(messages.len(), 2);
    let message_01 = messages[0].clone();
    let message_02 = messages[1].clone();
    drop(space);

    // Add new pull-only member to Space
    // ~~~~~~~~~~~~

    let space = manager.space(space_id).await.unwrap().unwrap();
    let (messages, _) = space.add(bob.manager.id(), Access::pull()).await.unwrap();
    let members = space.members().await.unwrap();

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
    let alice = TestPeer::new(0).await;
    let bob = TestPeer::new(1).await;

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

    let alice_id = alice.manager.id();
    let bob_id = bob.manager.id();

    let alice_manager = alice.manager.clone();
    let bob_manager = bob.manager.clone();

    // Alice: Create Space
    // ~~~~~~~~~~~~

    let space_id = 0;
    let (space, messages, _) = alice_manager.create_space(space_id, &[]).await.unwrap();
    let group_id = space.group_id().await.unwrap();
    drop(space);

    // Bob: Receive Message 01 & 02
    // ~~~~~~~~~~~~

    let message_01 = messages[0].clone();
    let message_02 = messages[1].clone();

    bob_manager.persist_message(&message_01).await.unwrap();
    bob_manager.process(&message_01).await.unwrap();

    // Global auth state has been updated.
    {
        let manager_ref = bob_manager.inner.read().await;
        let auth_y = manager_ref.store.auth().await.unwrap();
        let members = auth_y.members(group_id);
        assert_eq!(members, vec![(alice_id, Access::manage())]);
        assert_eq!(vec![message_01.id()], auth_y.orderer_y.heads());
    }

    bob_manager.persist_message(&message_02).await.unwrap();
    bob_manager.process(&message_02).await.unwrap();
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

    let (messages, _) = space.add(bob.manager.id(), Access::read()).await.unwrap();
    let message_04 = messages[0].clone();
    let message_05 = messages[1].clone();

    drop(space);

    // Bob: Receive Message 03, 04 and 05
    // ~~~~~~~~~~~~

    bob_manager.persist_message(&message_03).await.unwrap();
    let events = bob.manager.process(&message_03).await.unwrap();
    assert!(events.is_empty());
    bob_manager.persist_message(&message_04).await.unwrap();
    let _ = bob.manager.process(&message_04).await.unwrap();
    assert!(events.is_empty());
    bob_manager.persist_message(&message_05).await.unwrap();
    let events = bob.manager.process(&message_05).await.unwrap();
    // The application message arrives only after bob is welcomed.
    assert_eq!(events.len(), 2);
    assert!(matches!(events[1], Event::Application { .. }));

    let space = bob_manager.space(space_id).await.unwrap().unwrap();
    // Alice and bob are both members.
    let members = space.members().await.unwrap();
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
    let alice = TestPeer::new(0).await;
    let bob = TestPeer::new(1).await;

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

    let bob_id = bob.manager.id();

    let alice_manager = alice.manager.clone();
    let bob_manager = bob.manager.clone();

    // Alice: Create Space with themselves and bob
    // ~~~~~~~~~~~~

    let space_id = 0;
    let (space, messages, _) = alice_manager
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

    bob_manager.persist_message(&message_01).await.unwrap();
    let events = bob_manager.process(&message_01).await.unwrap();
    assert_eq!(events.len(), 1);
    bob_manager.persist_message(&message_02).await.unwrap();
    let events = bob_manager.process(&message_02).await.unwrap();
    assert_eq!(events.len(), 1);

    // Alice: Removes bob
    // ~~~~~~~~~~~~

    let space = alice_manager.space(space_id).await.unwrap().unwrap();
    let (messages, _) = space.remove(bob_id).await.unwrap();

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

    bob_manager.persist_message(&message_03).await.unwrap();
    let events = bob_manager.process(&message_03).await.unwrap();
    assert_eq!(events.len(), 1);
    bob_manager.persist_message(&message_04).await.unwrap();
    let events = bob_manager.process(&message_04).await.unwrap();
    assert_eq!(events.len(), 2);
    assert!(matches!(
        events[1],
        Event::Space(SpaceEvent::Ejected { .. })
    ));
}

#[tokio::test]
async fn concurrent_removal_conflict() {
    let alice = TestPeer::new(0).await;
    let bob = TestPeer::new(1).await;
    let claire = <TestPeer>::new(2).await;
    let dave = <TestPeer>::new(3).await;

    let alice_manager = alice.manager.clone();
    let bob_manager = bob.manager.clone();
    let claire_manager = claire.manager.clone();
    let dave_manager = dave.manager.clone();

    let alice_bundle = alice_manager.key_bundle_message().await.unwrap();
    let bob_bundle = bob_manager.key_bundle_message().await.unwrap();
    let claire_bundle = claire_manager.key_bundle_message().await.unwrap();
    let dave_bundle = dave_manager.key_bundle_message().await.unwrap();

    for bundle in [alice_bundle, bob_bundle, claire_bundle, dave_bundle] {
        alice_manager.process(&bundle).await.unwrap();
        bob_manager.process(&bundle).await.unwrap();
        claire_manager.process(&bundle).await.unwrap();
        dave_manager.process(&bundle).await.unwrap();
    }

    let bob_id = bob.manager.id();
    let claire_id = claire.manager.id();
    let dave_id = dave.manager.id();

    // Alice: Create Space with themselves and bob
    // ~~~~~~~~~~~~

    let space_id = 0;
    let (space, messages, _) = alice_manager
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

    bob_manager.persist_message(&message_01).await.unwrap();
    bob_manager.process(&message_01).await.unwrap();
    bob_manager.persist_message(&message_02).await.unwrap();
    bob_manager.process(&message_02).await.unwrap();

    // Alice: Removes bob (concurrently)
    // ~~~~~~~~~~~~

    let space = alice_manager.space(space_id).await.unwrap().unwrap();
    let _ = space.remove(bob_id).await.unwrap();
    drop(space);

    // Bob: Adds claire (concurrently)
    // ~~~~~~~~~~~~

    let space = bob_manager.space(space_id).await.unwrap().unwrap();
    let (messages, _) = space.add(claire_id, Access::read()).await.unwrap();
    drop(space);

    // There are two messages (one auth, and one space)
    assert_eq!(messages.len(), 2);
    let message_03 = messages[0].clone();
    let message_04 = messages[1].clone();

    // Alice: process bobs' message
    // ~~~~~~~~~~~~

    alice_manager.persist_message(&message_03).await.unwrap();
    alice_manager.process(&message_03).await.unwrap();
    alice_manager.persist_message(&message_04).await.unwrap();
    alice_manager.process(&message_04).await.unwrap();

    // Alice: Adds dave
    // ~~~~~~~~~~~~

    let space = alice_manager.space(space_id).await.unwrap().unwrap();
    let (messages, _) = space.add(dave_id, Access::read()).await.unwrap();

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
    let alice = TestPeer::new(0).await;
    let bob = <TestPeer>::new(1).await;
    let claire = <TestPeer>::new(2).await;

    let alice_id = alice.manager.id();
    let bob_id = bob.manager.id();
    let claire_id = claire.manager.id();

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

    let (group, messages, _) = alice_manager
        .create_group(&[(bob_id, Access::manage()), (claire_id, Access::manage())])
        .await
        .unwrap();
    let member_group_id = group.id();

    assert_eq!(messages.len(), 1);
    let message_01 = messages[0].clone();

    // Create Space with group as member
    // ~~~~~~~~~~~~

    let space_id = 0;
    let (space, messages, _) = alice_manager
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
    let members = space.members().await.unwrap();
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
    let alice = <TestPeer>::new(0).await;
    let bob = <TestPeer>::new(1).await;

    let alice_id = alice.manager.id();
    let bob_id = bob.manager.id();
    let manager = alice.manager.clone();

    // Create Group
    // ~~~~~~~~~~~~

    let (group, messages, _) = manager
        .create_group(&[(alice_id, Access::manage()), (bob_id, Access::manage())])
        .await
        .unwrap();

    assert_eq!(messages.len(), 1);
    let message_01 = messages[0].clone();

    let members = group.members().await.unwrap();
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
    let alice = <TestPeer>::new(0).await;
    let bob = <TestPeer>::new(1).await;
    let claire = <TestPeer>::new(2).await;

    let alice_id = alice.manager.id();
    let bob_id = bob.manager.id();
    let claire_id = claire.manager.id();
    let manager = alice.manager.clone();

    // Create Group
    // ~~~~~~~~~~~~

    let (group, messages, _) = manager
        .create_group(&[(alice_id, Access::manage()), (bob_id, Access::manage())])
        .await
        .unwrap();

    assert_eq!(messages.len(), 1);
    let message_01 = messages[0].clone();

    let (messages, _) = group.add(claire_id, Access::read()).await.unwrap();
    assert_eq!(messages.len(), 1);
    let message_02 = messages[0].clone();

    let members = group.members().await.unwrap();
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
    let alice = <TestPeer>::new(0).await;
    let bob = <TestPeer>::new(1).await;

    let alice_id = alice.manager.id();
    let bob_id = bob.manager.id();
    let manager = alice.manager.clone();

    // Create Group
    // ~~~~~~~~~~~~

    let (group, messages, _) = manager
        .create_group(&[(alice_id, Access::manage()), (bob_id, Access::manage())])
        .await
        .unwrap();
    assert_eq!(messages.len(), 1);
    let message_01 = messages[0].clone();

    // Remove bob from group
    // ~~~~~~~~~~~~

    let (messages, _) = group.remove(bob_id).await.unwrap();
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
    let alice = <TestPeer>::new(0).await;
    let bob = <TestPeer>::new(1).await;
    let claire = <TestPeer>::new(2).await;

    let alice_id = alice.manager.id();
    let bob_id = bob.manager.id();
    let claire_id = claire.manager.id();

    let alice_manager = alice.manager.clone();
    let bob_manager = bob.manager.clone();

    // Create Group
    // ~~~~~~~~~~~~

    let (group, messages, _) = alice_manager
        .create_group(&[(alice_id, Access::manage()), (bob_id, Access::manage())])
        .await
        .unwrap();
    let group_id = group.id();
    assert_eq!(messages.len(), 1);
    let message_01 = messages[0].clone();

    // Add claire
    // ~~~~~~~~~~~~

    let (messages, _) = group.add(claire_id, Access::read()).await.unwrap();
    assert_eq!(messages.len(), 1);
    let message_02 = messages[0].clone();
    drop(group);

    // Bob receives message 01 & 02
    // ~~~~~~~~~~~~

    let _events = bob_manager.process(&message_01).await.unwrap();
    let _events = bob_manager.process(&message_02).await.unwrap();

    let group = bob_manager.group(group_id).await.unwrap().unwrap();
    let members = group.members().await.unwrap();
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
    let alice = TestPeer::new(0).await;
    let bob = <TestPeer>::new(1).await;
    let claire = <TestPeer>::new(2).await;

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

    let alice_id = alice.manager.id();
    let bob_id = bob.manager.id();
    let claire_id = claire.manager.id();

    let manager = alice.manager.clone();

    // Create Space 0
    // ~~~~~~~~~~~~

    let space_id = 0;
    let (space_0, messages, _) = manager
        .create_space(space_id, &[(alice_id, Access::manage())])
        .await
        .unwrap();

    assert_eq!(messages.len(), 2);

    // Create Space 1
    // ~~~~~~~~~~~~

    let space_id = 1;
    let (space_1, messages, _) = manager
        .create_space(space_id, &[(alice_id, Access::manage())])
        .await
        .unwrap();
    // There are four messages (one auth, and three space)
    assert_eq!(messages.len(), 4);

    // Create group
    // ~~~~~~~~~~~~

    let (group, messages, _) = manager
        .create_group(&[(alice_id, Access::manage()), (bob_id, Access::read())])
        .await
        .unwrap();

    // There are three messages (one auth, and two space)
    assert_eq!(messages.len(), 3);

    // Add group to space 0
    // ~~~~~~~~~~~~

    let (messages, _) = space_0.add(group.id(), Access::read()).await.unwrap();
    // There are three messages (one auth, and two space)
    assert_eq!(messages.len(), 3);

    // Add group to space 1
    // ~~~~~~~~~~~~

    let (messages, _) = space_1.add(group.id(), Access::read()).await.unwrap();
    // There are three messages (one auth, and two space)
    assert_eq!(messages.len(), 3);

    // Add claire to the group
    // ~~~~~~~~~~~~

    let (messages, _) = group.add(claire_id, Access::read()).await.unwrap();
    // There are three messages (one auth, and two space)
    assert_eq!(messages.len(), 3);

    // Both space 0 and space 1 should now include claire.
    let expected_members = vec![
        (alice_id, Access::manage()),
        (bob_id, Access::read()),
        (claire_id, Access::read()),
    ];

    let members = space_0.members().await.unwrap();
    assert_eq!(expected_members, members);

    let members = space_1.members().await.unwrap();
    assert_eq!(expected_members, members);
}

#[tokio::test]
async fn events() {
    let alice = TestPeer::new(0).await;
    let bob = <TestPeer>::new(1).await;
    let claire = <TestPeer>::new(2).await;
    let dave = <TestPeer>::new(3).await;

    let alice_id = alice.manager.id();
    let bob_id = bob.manager.id();
    let claire_id = claire.manager.id();
    let dave_id = dave.manager.id();

    let alice_manager = alice.manager.clone();
    let bob_manager = bob.manager.clone();
    let claire_manager = claire.manager.clone();
    let dave_manager = dave.manager.clone();

    let alice_bundle = alice_manager.key_bundle_message().await.unwrap();
    let bob_bundle = bob_manager.key_bundle_message().await.unwrap();
    let claire_bundle = claire_manager.key_bundle_message().await.unwrap();
    let dave_bundle = dave_manager.key_bundle_message().await.unwrap();

    for bundle in [alice_bundle, bob_bundle, claire_bundle, dave_bundle] {
        alice_manager.process(&bundle).await.unwrap();
        bob_manager.process(&bundle).await.unwrap();
        claire_manager.process(&bundle).await.unwrap();
        dave_manager.process(&bundle).await.unwrap();
    }

    let mut alice_messages = vec![];
    let mut alice_events = vec![];

    // Create Group with bob and claire as managers.
    let (group, messages, event) = alice_manager
        .create_group(&[(bob_id, Access::manage()), (claire_id, Access::manage())])
        .await
        .unwrap();
    let member_group_id = group.id();

    assert_eq!(messages.len(), 1);
    alice_messages.extend(messages);
    alice_events.push(event);

    // Create Space with group as member
    let space_id = 0;
    let (space, messages, events) = alice_manager
        .create_space(space_id, &[(member_group_id, Access::read())])
        .await
        .unwrap();
    let space_group_id = space.group_id().await.unwrap();
    assert_eq!(messages.len(), 3);
    alice_messages.extend(messages);
    assert_eq!(events.len(), 2);
    alice_events.extend(events);

    // Add dave to space with read access
    let (messages, events) = space.add(dave_id, Access::read()).await.unwrap();
    assert_eq!(messages.len(), 2);
    alice_messages.extend(messages);
    assert_eq!(events.len(), 2);
    alice_events.extend(events);

    // Remove dave from space
    let (messages, events) = space.remove(dave_id).await.unwrap();
    assert_eq!(messages.len(), 2);
    alice_messages.extend(messages);
    assert_eq!(events.len(), 2);
    alice_events.extend(events);

    // Add dave back into space with pull access
    let (messages, events) = space.add(dave_id, Access::pull()).await.unwrap();
    assert_eq!(messages.len(), 2);
    alice_messages.extend(messages);
    // Only one event as new member only has Pull access so no space membership change occurred
    // and therefore no space event was emitted.
    assert_eq!(events.len(), 1);
    alice_events.extend(events);

    // Remove member group from space
    let (messages, events) = space.remove(group.id()).await.unwrap();
    assert_eq!(messages.len(), 2);
    alice_messages.extend(messages);
    assert_eq!(events.len(), 2);
    alice_events.extend(events);

    // Test basic expected event types.
    let mut all_bob_events = vec![];
    for (idx, message) in alice_messages.iter().enumerate() {
        bob_manager.persist_message(&message).await.unwrap();
        let bob_events = bob_manager.process(&message).await.unwrap();
        all_bob_events.extend(bob_events.clone());
        match idx {
            // Member auth group created.
            0 => {
                assert_eq!(bob_events.len(), 1);
                assert_matches!(bob_events[0].clone(), Event::Group(GroupEvent::Created { group_id, .. }) if group_id == group.id());
                assert_matches!(bob_events[0].clone(), Event::Group(GroupEvent::Created { context: GroupContext{ author, .. }, .. }) if author == alice_id);
            }
            // Space auth group created.
            1 => {
                assert_eq!(bob_events.len(), 1);
                assert_matches!(bob_events[0].clone(), Event::Group(GroupEvent::Created { group_id, .. }) if group_id == space_group_id);
                assert_matches!(bob_events[0].clone(), Event::Group(GroupEvent::Created { context: GroupContext{ author, .. }, .. }) if author == alice_id);
            }
            // Both previous auth messages published to newly created space, initial members added
            // to encryption context.
            2 => assert_eq!(bob_events.len(), 0),
            3 => {
                assert_eq!(bob_events.len(), 1);
                assert_matches!(bob_events[0].clone(), Event::Space(SpaceEvent::Created { space_id, context: SpaceContext{group_id, ..}, .. }) if space_id == space.id() && group_id == space_group_id);
                assert_matches!(bob_events[0].clone(), Event::Space(SpaceEvent::Created { context: SpaceContext{ auth_author, spaces_author, ..}, .. }) if auth_author == alice_id && spaces_author == alice_id);
            }
            // Dave added to space auth group and encryption context.
            4 => {
                assert_eq!(bob_events.len(), 1);
                assert_matches!(bob_events[0].clone(), Event::Group(GroupEvent::Added { added, group_id, .. }) if added == GroupActor::individual(dave_id) && group_id == space_group_id);
                assert_matches!(bob_events[0].clone(), Event::Group(GroupEvent::Added { context: GroupContext{ author, .. }, .. }) if author == alice_id);
            }
            5 => {
                assert_eq!(bob_events.len(), 1);
                assert_matches!(bob_events[0].clone(), Event::Space(SpaceEvent::Added { added, .. }) if added == vec![dave_id]);
                assert_matches!(bob_events[0].clone(), Event::Space(SpaceEvent::Added { context: SpaceContext{ auth_author, spaces_author, ..}, .. }) if auth_author == alice_id && spaces_author == alice_id);
            }
            // Dave removed from space auth group and encryption context.
            6 => {
                assert_eq!(bob_events.len(), 1);
                assert_matches!(bob_events[0].clone(), Event::Group(GroupEvent::Removed { removed, .. }) if removed == GroupActor::individual(dave_id));
                assert_matches!(bob_events[0].clone(), Event::Group(GroupEvent::Removed { context: GroupContext{ author, .. }, .. }) if author == alice_id);
            }
            7 => {
                assert_eq!(bob_events.len(), 1);
                assert_matches!(bob_events[0].clone(), Event::Space(SpaceEvent::Removed { removed, .. }) if removed == vec![dave_id]);
                assert_matches!(bob_events[0].clone(), Event::Space(SpaceEvent::Removed { context: SpaceContext{ auth_author, spaces_author, ..}, .. }) if auth_author == alice_id && spaces_author == alice_id);
            }
            // Dave added to auth group with pull access and no resulting encryption context change.
            8 => {
                assert_eq!(bob_events.len(), 1);
                assert_matches!(bob_events[0].clone(), Event::Group(GroupEvent::Added { added, .. }) if added == GroupActor::individual(dave_id));
                assert_matches!(bob_events[0].clone(), Event::Group(GroupEvent::Added { context: GroupContext{ author, .. }, .. }) if author == alice_id);
            }
            9 => assert_eq!(bob_events.len(), 0, "{:?}", bob_events),
            // Remove member group from space auth group, both bob and claire removed from space
            // encryption context.
            10 => {
                assert_eq!(bob_events.len(), 1);
                assert_matches!(bob_events[0].clone(), Event::Group(GroupEvent::Removed { removed, .. }) if removed == GroupActor::group(group.id()));
                assert_matches!(bob_events[0].clone(), Event::Group(GroupEvent::Removed { context: GroupContext{ author, .. }, .. }) if author == alice_id);
            }
            11 => {
                assert_eq!(bob_events.len(), 2);
                assert_matches!(bob_events[0].clone(), Event::Space(SpaceEvent::Removed { removed, .. }) if removed == vec![bob_id, claire_id]);
                assert_matches!(bob_events[0].clone(), Event::Space(SpaceEvent::Removed { context: SpaceContext{ auth_author, spaces_author, ..}, .. }) if auth_author == alice_id && spaces_author == alice_id);
                assert_matches!(
                    bob_events[1].clone(),
                    Event::Space(SpaceEvent::Ejected { .. })
                );
            }
            _ => panic!(),
        }
    }

    assert_eq!(alice_events.len(), 10);
    // Bob has one more event as he got the "ejected" event when being removed from the space.
    assert_eq!(all_bob_events.len(), 11);
    all_bob_events.pop();

    for (id, bob_event) in all_bob_events.iter().enumerate() {
        assert_eq!(&alice_events[id], bob_event)
    }

    // Test expected members.
    for (idx, event) in alice_events.iter().enumerate() {
        match idx {
            // Member auth group created.
            0 => {
                let Event::Group(GroupEvent::Created {
                    context:
                        GroupContext {
                            group_actors,
                            members,
                            ..
                        },
                    ..
                }) = event.clone()
                else {
                    panic!()
                };

                let expected_group_actors = vec![
                    (GroupActor::individual(bob_id), Access::manage()),
                    (GroupActor::individual(claire_id), Access::manage()),
                ];

                let expected_members =
                    vec![(bob_id, Access::manage()), (claire_id, Access::manage())];

                assert_eq!(group_actors, expected_group_actors);
                assert_eq!(members, expected_members);
            }
            // Space auth group created.
            1 => {
                let Event::Group(GroupEvent::Created {
                    context:
                        GroupContext {
                            group_actors,
                            members,
                            ..
                        },
                    ..
                }) = event.clone()
                else {
                    panic!()
                };

                let mut expected_group_actors = vec![
                    (GroupActor::individual(alice_id), Access::manage()),
                    (GroupActor::group(group.id()), Access::read()),
                ];
                let expected_members = vec![
                    (alice_id, Access::manage()),
                    (bob_id, Access::read()),
                    (claire_id, Access::read()),
                ];

                sort_group_actors(&mut expected_group_actors);
                assert_eq!(group_actors, expected_group_actors);
                assert_eq!(members, expected_members);
            }
            // Both previous auth messages published to newly created space, initial members added
            // to encryption context.
            2 => {
                let Event::Space(SpaceEvent::Created {
                    context: SpaceContext { members, .. },
                    ..
                }) = event.clone()
                else {
                    panic!()
                };

                let expected_members = vec![alice_id, bob_id, claire_id];
                assert_eq!(members, expected_members);
            }
            // Dave added to space auth group and encryption context.
            3 => {
                let Event::Group(GroupEvent::Added {
                    context:
                        GroupContext {
                            group_actors,
                            members,
                            ..
                        },
                    ..
                }) = event.clone()
                else {
                    panic!()
                };

                let mut expected_group_actors = vec![
                    (GroupActor::individual(alice_id), Access::manage()),
                    (GroupActor::individual(dave_id), Access::read()),
                    (GroupActor::group(group.id()), Access::read()),
                ];
                let mut expected_members = vec![
                    (alice_id, Access::manage()),
                    (bob_id, Access::read()),
                    (claire_id, Access::read()),
                    (dave_id, Access::read()),
                ];

                sort_group_actors(&mut expected_group_actors);
                sort_members(&mut expected_members);
                assert_eq!(group_actors, expected_group_actors);
                assert_eq!(members, expected_members);
            }
            4 => {
                let Event::Space(SpaceEvent::Added {
                    context: SpaceContext { members, .. },
                    ..
                }) = event.clone()
                else {
                    panic!()
                };

                let mut expected_members = vec![alice_id, bob_id, claire_id, dave_id];
                expected_members.sort();
                assert_eq!(members, expected_members);
            }
            // Dave removed from space auth group and encryption context.
            5 => {
                let Event::Group(GroupEvent::Removed {
                    context:
                        GroupContext {
                            group_actors,
                            members,
                            ..
                        },
                    ..
                }) = event.clone()
                else {
                    panic!()
                };

                let mut expected_group_actors = vec![
                    (GroupActor::individual(alice_id), Access::manage()),
                    (GroupActor::group(group.id()), Access::read()),
                ];
                let mut expected_members = vec![
                    (alice_id, Access::manage()),
                    (bob_id, Access::read()),
                    (claire_id, Access::read()),
                ];

                sort_group_actors(&mut expected_group_actors);
                sort_members(&mut expected_members);
                assert_eq!(group_actors, expected_group_actors);
                assert_eq!(members, expected_members);
            }
            6 => {
                let Event::Space(SpaceEvent::Removed {
                    context: SpaceContext { members, .. },
                    ..
                }) = event.clone()
                else {
                    panic!()
                };

                let mut expected_members = vec![alice_id, bob_id, claire_id];
                expected_members.sort();
                assert_eq!(members, expected_members);
            }
            // Dave added to auth group with pull access and no resulting encryption context change.
            7 => {
                let Event::Group(GroupEvent::Added {
                    context:
                        GroupContext {
                            group_actors,
                            members,
                            ..
                        },
                    ..
                }) = event.clone()
                else {
                    panic!()
                };

                let mut expected_group_actors = vec![
                    (GroupActor::individual(alice_id), Access::manage()),
                    (GroupActor::individual(dave_id), Access::pull()),
                    (GroupActor::group(group.id()), Access::read()),
                ];
                let mut expected_members = vec![
                    (alice_id, Access::manage()),
                    (bob_id, Access::read()),
                    (claire_id, Access::read()),
                    (dave_id, Access::pull()),
                ];

                sort_group_actors(&mut expected_group_actors);
                sort_members(&mut expected_members);
                assert_eq!(group_actors, expected_group_actors);
                assert_eq!(members, expected_members);
            }
            // Remove member group from space auth group, both bob and claire removed from space
            // encryption context.
            8 => {
                let Event::Group(GroupEvent::Removed {
                    context:
                        GroupContext {
                            group_actors,
                            members,
                            ..
                        },
                    ..
                }) = event.clone()
                else {
                    panic!()
                };

                let mut expected_group_actors = vec![
                    (GroupActor::individual(alice_id), Access::manage()),
                    (GroupActor::individual(dave_id), Access::pull()),
                ];
                let mut expected_members =
                    vec![(alice_id, Access::manage()), (dave_id, Access::pull())];

                sort_group_actors(&mut expected_group_actors);
                sort_members(&mut expected_members);
                assert_eq!(group_actors, expected_group_actors);
                assert_eq!(members, expected_members);
            }
            9 => {
                let Event::Space(SpaceEvent::Removed {
                    context: SpaceContext { members, .. },
                    ..
                }) = event.clone()
                else {
                    panic!()
                };

                let mut expected_members = vec![alice_id];
                expected_members.sort();
                assert_eq!(members, expected_members);
            }
            _ => panic!(),
        }
    }
}

#[tokio::test]
async fn idempotent_api() {
    let alice = TestPeer::new(0).await;
    let bob = TestPeer::new(1).await;

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

    let alice_manager = alice.manager.clone();
    let bob_manager = bob.manager.clone();

    let alice_id = alice_manager.id();

    // Alice: Create Space
    // ~~~~~~~~~~~~

    let space_id = 0;
    let (_, messages, _) = alice_manager.create_space(space_id, &[]).await.unwrap();

    let message_01 = messages[0].clone();
    let message_02 = messages[1].clone();

    // Alice can process both messages again, no state should change, and no events should be
    // returned.
    let events = alice_manager.process(&message_01).await.unwrap();
    assert!(events.is_empty());
    let events = alice_manager.process(&message_02).await.unwrap();
    assert!(events.is_empty());
    let space = alice_manager.space(space_id).await.unwrap().unwrap();
    let members = space.members().await.unwrap();
    assert_eq!(members, vec![(alice_id, Access::manage()),]);

    // Bob: Receive Message 01 & 02
    // ~~~~~~~~~~~~

    bob_manager.persist_message(&message_01).await.unwrap();
    bob_manager.process(&message_01).await.unwrap();
    bob_manager.persist_message(&message_02).await.unwrap();
    bob_manager.process(&message_02).await.unwrap();

    // Bob can process both messages again, no state should change, and no events should be
    // returned.
    let events = bob_manager.process(&message_01).await.unwrap();
    assert!(events.is_empty());
    let events = bob_manager.process(&message_02).await.unwrap();
    assert!(events.is_empty());
    let space = bob_manager.space(space_id).await.unwrap().unwrap();
    let members = space.members().await.unwrap();
    assert_eq!(members, vec![(alice_id, Access::manage()),]);
}

#[tokio::test]
async fn repair_space() {
    let alice = TestPeer::new(0).await;
    let bob = <TestPeer>::new(1).await;
    let claire = <TestPeer>::new(2).await;

    let alice_id = alice.manager.id();
    let bob_id = bob.manager.id();
    let claire_id = claire.manager.id();

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

    // Manually register all key bundles on bob.

    bob_manager
        .register_member(&alice_manager.me().await.unwrap())
        .await
        .unwrap();

    bob_manager
        .register_member(&claire_manager.me().await.unwrap())
        .await
        .unwrap();

    // Alice: Create Group with Bob as a manager.
    // ~~~~~~~~~~~~

    let (group, messages, _) = alice_manager
        .create_group(&[(bob_id, Access::manage())])
        .await
        .unwrap();
    let member_group_id = group.id();

    assert_eq!(messages.len(), 1);
    let message_01 = messages[0].clone();

    // Alice: Create Space with group as member.
    // ~~~~~~~~~~~~

    let space_id = 0;
    let (space, messages, _) = alice_manager
        .create_space(space_id, &[(member_group_id, Access::read())])
        .await
        .unwrap();
    drop(space);

    let message_02 = messages[0].clone();
    let message_03 = messages[1].clone();
    let message_04 = messages[2].clone();

    // Bob: Process message_01 (create group) and add a member to the group without learning about any space yet.
    // ~~~~~~~~~~~~

    bob_manager.persist_message(&message_01).await.unwrap();
    bob_manager.process(&message_01).await.unwrap();
    let group = bob_manager.group(member_group_id).await.unwrap().unwrap();
    let (messages, _) = group.add(claire_id, Access::read()).await.unwrap();
    let bob_message_01 = messages[0].clone();
    drop(group);

    // Alice: Process Bob's message (published concurrently to the space creation).
    // ~~~~~~~~~~~~

    alice_manager
        .persist_message(&bob_message_01)
        .await
        .unwrap();
    alice_manager.process(&bob_message_01).await.unwrap();

    // Detect if any spaces require repairing.
    let repair_required = alice_manager.spaces_repair_required().await.unwrap();
    assert_eq!(vec![space_id], repair_required);

    // Trigger repair of the space.
    let (messages, _events) = alice_manager.repair_spaces(&repair_required).await.unwrap();
    let message_05 = messages[0].clone();
    // @TODO: assert the correct events are present once we return events for messages we created ourselves.

    // Alice's space members now contain Claire (the space was repaired).
    let space = alice_manager.space(space_id).await.unwrap().unwrap();
    let mut members = space.members().await.unwrap();
    members.sort_by(|(actor_a, _), (actor_b, _)| actor_a.cmp(actor_b));
    let expected_members = vec![
        (alice_id, Access::manage()),
        (bob_id, Access::read()),
        (claire_id, Access::read()),
    ];
    assert_eq!(members, expected_members);
    drop(space);

    // Bob: process all Alice's remaining messages (including the message repairing the space).
    // ~~~~~~~~~~~~

    // Persist and process all messages from alice.
    for message in [message_02, message_03, message_04, message_05] {
        bob_manager.persist_message(&message).await.unwrap();
        bob_manager.process(&message).await.unwrap();
    }

    // Bob now knows about the space and has correct members.
    let space = bob_manager.space(space_id).await.unwrap().unwrap();
    let mut members = space.members().await.unwrap();
    members.sort_by(|(actor_a, _), (actor_b, _)| actor_a.cmp(actor_b));
    assert_eq!(members, expected_members);
}

#[tokio::test]
async fn duplicate_auth_state_references() {
    let alice = TestPeer::new(0).await;
    let bob = <TestPeer>::new(1).await;
    let claire = <TestPeer>::new(2).await;

    let alice_id = alice.manager.id();
    let bob_id = bob.manager.id();
    let claire_id = claire.manager.id();

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

    // Manually register all key bundles on bob.

    bob_manager
        .register_member(&alice_manager.me().await.unwrap())
        .await
        .unwrap();

    bob_manager
        .register_member(&claire_manager.me().await.unwrap())
        .await
        .unwrap();

    // Alice: Create Group with Bob as a manager.
    // ~~~~~~~~~~~~

    let (group, messages, _) = alice_manager
        .create_group(&[(bob_id, Access::manage())])
        .await
        .unwrap();
    let member_group_id = group.id();

    assert_eq!(messages.len(), 1);
    let message_01 = messages[0].clone();

    // Alice: Create Space with group as member.
    // ~~~~~~~~~~~~

    let space_id = 0;
    let (space, messages, _) = alice_manager
        .create_space(space_id, &[(member_group_id, Access::read())])
        .await
        .unwrap();
    drop(space);

    let message_02 = messages[0].clone();
    let message_03 = messages[1].clone();
    let message_04 = messages[2].clone();

    // Bob: Process message_01 (create group) and add a member to the group without learning about any space yet.
    // ~~~~~~~~~~~~

    bob_manager.persist_message(&message_01).await.unwrap();
    bob_manager.process(&message_01).await.unwrap();
    let group = bob_manager.group(member_group_id).await.unwrap().unwrap();
    let (messages, _) = group.add(claire_id, Access::read()).await.unwrap();
    let bob_message_01 = messages[0].clone();
    drop(group);

    // Alice: Process Bob's message (published concurrently to the space creation).
    // ~~~~~~~~~~~~

    alice_manager
        .persist_message(&bob_message_01)
        .await
        .unwrap();
    alice_manager.process(&bob_message_01).await.unwrap();

    // Detect if any spaces require repairing.
    let repair_required = alice_manager.spaces_repair_required().await.unwrap();
    assert_eq!(vec![space_id], repair_required);

    // Trigger repair of the space.
    let (messages, _) = alice_manager.repair_spaces(&repair_required).await.unwrap();
    let message_05 = messages[0].clone();
    // @TODO: assert the correct events are present once we return events for messages we created ourselves.

    // Alice's space members now contain Claire (the space was repaired).
    let space = alice_manager.space(space_id).await.unwrap().unwrap();
    let members = space.members().await.unwrap();
    let expected_members = vec![
        (alice_id, Access::manage()),
        (bob_id, Access::read()),
        (claire_id, Access::read()),
    ];
    assert_eq!(members, expected_members);
    drop(space);

    // Bob: processes Alice's messages except the "auth pointer" published to repair the space.
    // ~~~~~~~~~~~~

    for message in [message_02, message_03, message_04] {
        bob_manager.persist_message(&message).await.unwrap();
        bob_manager.process(&message).await.unwrap();
    }

    // Bob: repair the space (as alice's auth pointer not yet received)
    // ~~~~~~~~~~~~

    // Detect if any spaces require repairing.
    let repair_required = bob_manager.spaces_repair_required().await.unwrap();
    assert_eq!(vec![space_id], repair_required);

    // Trigger repair of the space.
    let (messages, _) = bob_manager.repair_spaces(&repair_required).await.unwrap();
    let _ = messages[0].clone();

    // Bob: processes Alice's (duplicate) auth state pointer.
    // ~~~~~~~~~~~~

    bob_manager.persist_message(&message_05).await.unwrap();
    bob_manager.process(&message_05).await.unwrap();

    // Bob arrived at the expected state without error.
    let space = bob_manager.space(space_id).await.unwrap().unwrap();
    let members = space.members().await.unwrap();
    assert_eq!(members, expected_members);
}

#[tokio::test]
async fn key_store_expired() {
    let peer = TestPeer::new(0).await;

    // Any just created instance will need a pre-key in the beginning.
    assert!(peer.manager.key_bundle_expired().await.unwrap());

    // Publish a new key bundle with newly generated pre-keys and we should be fine.
    let _message = peer.manager.key_bundle_message().await.unwrap();
    assert!(!peer.manager.key_bundle_expired().await.unwrap());
}

#[tokio::test]
async fn add_expired_member_to_group() {
    let alice = TestPeer::new(0).await;
    let bob = TestPeer::new(1).await;

    // Create key bundle which expires in 1 second.
    let expired_bob = {
        let rng = Rng::from_seed([2; 32]);

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before unix epoch")
            .as_secs();

        // Generate pre-key & sign it.
        let prekey_secret = SecretKey::from_rng(&rng).unwrap();
        let prekey = PreKey::new(
            prekey_secret.public_key().unwrap(),
            Lifetime::from_range(now - 60, now + 1),
        );
        let signature = prekey
            .sign(&bob.credentials.identity_secret(), &rng)
            .unwrap();

        // Wrap it in key bundle.
        let bundle = LongTermKeyBundle::new(
            bob.credentials.identity_secret().public_key().unwrap(),
            prekey,
            signature,
        );

        Member::new(bob.manager.id(), bundle)
    };

    // Alice adds Bob's key bundle. At this point it is still valid.
    alice.manager.register_member(&expired_bob).await.unwrap();

    // Sleep to make bundle expire.
    thread::sleep(Duration::from_secs(1));

    // Alice creates a space with Bob but it should fail since the key bundle has expired.
    assert!(
        alice
            .manager
            .create_space(0, &[(expired_bob.id(), Access::write())])
            .await
            .is_err()
    );
}

#[tokio::test]
async fn process_operation_from_expired_member() {
    let alice = TestPeer::new(0).await;
    let bob = TestPeer::new(1).await;

    // Create key bundle which expires in 1 second.
    let expired_bob = {
        let rng = Rng::from_seed([2; 32]);

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before unix epoch")
            .as_secs();

        // Generate pre-key & sign it.
        let prekey_secret = SecretKey::from_rng(&rng).unwrap();
        let prekey = PreKey::new(
            prekey_secret.public_key().unwrap(),
            Lifetime::from_range(now - 60, now + 1),
        );
        let signature = prekey
            .sign(&bob.credentials.identity_secret(), &rng)
            .unwrap();

        // Wrap it in key bundle.
        let bundle = LongTermKeyBundle::new(
            bob.credentials.identity_secret().public_key().unwrap(),
            prekey,
            signature,
        );

        Member::new(bob.manager.id(), bundle)
    };

    // Alice adds Bob's key bundle. At this point it is still valid.
    alice.manager.register_member(&expired_bob).await.unwrap();

    // Bob should register it's own soon-invalid key bundle.
    bob.manager.register_member(&expired_bob).await.unwrap();

    // Alice creates a space with Bob.
    let (_space, messages, _events) = alice
        .manager
        .create_space(0, &[(expired_bob.id(), Access::write())])
        .await
        .unwrap();

    // Sleep to make bundle expire.
    thread::sleep(Duration::from_secs(2));

    // Bob processes Alice's "create group" message.
    bob.manager.process(&messages[0]).await.unwrap();
    // Bob processes Alice's "create space", but unfortunately Bob's key bundle expired and they
    // can't decrypt the initial key agreement (X3DH) in the direct message anymore.
    assert!(bob.manager.process(&messages[1]).await.is_err());
}
