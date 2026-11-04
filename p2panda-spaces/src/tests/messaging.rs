// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_auth::Access;

use crate::test_utils::TestPeer;
use crate::tests::spaces::{split_messages, split_space_output};
use crate::{Event, SpaceId};

#[tokio::test]
async fn receive_space_history_after_add() {
    let alice = TestPeer::new(0).await;
    let bob = <TestPeer>::new(1).await;

    let bob_id = bob.manager.id();

    let alice_manager = alice.manager.clone();
    let bob_manager = bob.manager.clone();

    let alice_bundle = alice_manager.rotate_key_bundle().await.unwrap().unwrap();
    let bob_bundle = bob_manager.rotate_key_bundle().await.unwrap().unwrap();

    for bundle in [alice_bundle, bob_bundle] {
        alice_manager.process(&bundle).await.unwrap();
        bob_manager.process(&bundle).await.unwrap();
    }

    let mut alice_messages = vec![];

    // Alice creates a space.
    let space_id = SpaceId::digest(b"0");
    let (space, output) = alice_manager.create_space(space_id, &[]).await.unwrap();
    let (messages, _) = split_messages(output.messages);
    alice_messages.extend(messages);

    // Alice publishes two messages before Dave is added to the space.
    let (message, _) = space.publish("Hello".as_bytes()).await.unwrap();
    alice_messages.push(message);
    let (message, _) = space.publish("Anyone there?".as_bytes()).await.unwrap();
    alice_messages.push(message);

    // Alice adds dave to space.
    let (auth_message, space_message, _) =
        split_space_output(space.add(bob_id, Access::read()).await.unwrap());
    alice_messages.extend([auth_message, space_message]);

    // Alice publishes a message after Dave was added.
    let (message, _) = space.publish("Feeling less lonely :-)".as_bytes()).await.unwrap();
    alice_messages.push(message);

    // Dave processes all Alice's control & application messages.
    let mut bob_events = vec![];
    for message in alice_messages.iter() {
        bob.persist_operation(&message).await.unwrap();
        let events = bob_manager.process(message).await.unwrap();
        bob_events.extend(events);
    }

    // We expect Dave to receive messages both from before and after he was added to the space.
    let application_events = bob_events
        .iter()
        .filter(|event| matches!(event, Event::Application { .. }))
        .collect::<Vec<_>>();
    assert_eq!(application_events.len(), 3);
}
