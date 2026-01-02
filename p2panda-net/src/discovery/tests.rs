// SPDX-License-Identifier: MIT OR Apache-2.0

use tokio::task::JoinHandle;

use crate::discovery::{DiscoveryEvent, SessionRole};
use crate::test_utils::{TestNode, setup_logging};

async fn session_ended_handle(node: &TestNode) -> JoinHandle<()> {
    let mut events = node.discovery.events().await.unwrap();

    tokio::spawn(async move {
        loop {
            let event = events.recv().await.unwrap();
            if let DiscoveryEvent::SessionEnded {
                role: SessionRole::Initiated,
                ..
            } = event
            {
                break;
            }
        }
    })
}

#[tokio::test]
async fn smoke_test() {
    setup_logging();

    // Spawn nodes.
    let alice = TestNode::spawn([7; 32]).await;
    let mut bob = TestNode::spawn([8; 32]).await;

    // Alice inserts Bob's info in their address book. Bob's address book is empty;
    alice
        .address_book
        .insert_node_info(bob.node_info())
        .await
        .unwrap();

    // Wait until both parties finished at least one discovery session.
    let alice_session_ended = session_ended_handle(&alice).await;
    let bob_session_ended = session_ended_handle(&bob).await;
    alice_session_ended.await.unwrap();
    bob_session_ended.await.unwrap();

    // Alice didn't learn about new transport info of Bob as their manually added node info was
    // already the "latest".
    let alice_metrics = alice.discovery.metrics().await.unwrap();
    assert_eq!(alice_metrics.newly_learned_transport_infos, 0);

    // Bob learned of Alice.
    let bob_metrics = bob.discovery.metrics().await.unwrap();
    assert_eq!(bob_metrics.newly_learned_transport_infos, 1);

    // Alice should now be in the address book of Bob.
    let result = bob.address_book.node_info(alice.node_id()).await.unwrap();
    assert!(result.is_some());
}
