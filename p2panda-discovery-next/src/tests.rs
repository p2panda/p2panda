// SPDX-License-Identifier: MIT OR Apache-2.0

use rand::SeedableRng;
use rand_chacha::ChaCha20Rng;
use tokio::sync::mpsc;

use crate::DiscoveryProtocol;
use crate::naive::NaiveDiscoveryProtocol;
use crate::random_walk::RandomWalk;
use crate::test_utils::{TestId, TestInfo, TestStore, TestSubscription, TestTopic};

#[tokio::test]
async fn naive_protocol() {
    let alice = 0;
    let bob = 1;

    // Initialize "Alice"

    let alice_rng = ChaCha20Rng::from_seed([1; 32]);
    let alice_store = TestStore::new(alice_rng.clone());

    let alice_subscription = TestSubscription {
        topics: vec![],
        topic_ids: vec![],
    };
    let alice_strategy = RandomWalk::new(alice_store.clone(), alice_rng);
    let alice_protocol = NaiveDiscoveryProtocol::<_, _, TestTopic, TestId, TestInfo>::new(
        alice_store,
        alice_subscription,
        bob,
    );

    // Initialize "Bob"

    let bob_rng = ChaCha20Rng::from_seed([2; 32]);
    let bob_store = TestStore::new(bob_rng.clone());

    let bob_subscription = TestSubscription {
        topics: vec![],
        topic_ids: vec![],
    };
    let bob_strategy = RandomWalk::new(bob_store.clone(), bob_rng);
    let bob_protocol = NaiveDiscoveryProtocol::<_, _, TestTopic, TestId, TestInfo>::new(
        bob_store,
        bob_subscription,
        alice,
    );

    // Start Protocol

    let (alice_tx, alice_rx) = mpsc::channel(16);
    let (bob_tx, bob_rx) = mpsc::channel(16);

    let bob_handle = tokio::spawn(async move { bob_protocol.bob(bob_tx, alice_rx).await });

    let alice_result = alice_protocol.alice(alice_tx, bob_rx).await.unwrap();

    let bob_result = bob_handle.await.unwrap().unwrap();

    println!("{:?}", alice_result);
    println!("{:?}", bob_result);
}
