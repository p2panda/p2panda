// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashMap;
use std::sync::Arc;

use rand::SeedableRng;
use rand_chacha::ChaCha20Rng;
use tokio::sync::{RwLock, mpsc};
use tokio::task::{JoinSet, LocalSet};

use crate::address_book::AddressBookStore;
use crate::naive::NaiveDiscoveryProtocol;
use crate::random_walk::RandomWalk;
use crate::test_utils::{TestId, TestInfo, TestStore, TestSubscription, TestTopic};
use crate::traits::{DiscoveryProtocol, DiscoveryStrategy};

struct TestNode {
    id: TestId,
    subscription: TestSubscription,
    store: TestStore<ChaCha20Rng>,
    strategy: RandomWalk<ChaCha20Rng, TestStore<ChaCha20Rng>, TestTopic, TestId, TestInfo>,
}

impl TestNode {
    pub fn new(id: TestId) -> Self {
        let rng = ChaCha20Rng::from_seed([1; 32]);
        let store = TestStore::new(rng.clone());

        let subscription = TestSubscription::default();
        let strategy = RandomWalk::new(store.clone(), rng);

        Self {
            id,
            subscription,
            store,
            strategy,
        }
    }

    pub async fn next_node(&self) -> Option<TestId> {
        match self.strategy.next_node().await.unwrap() {
            Some(info) => Some(info.id),
            None => None,
        }
    }

    pub async fn connect<P>(&self, alice_protocol: P, bob_protocol: P, remote: &TestNode)
    where
        P: DiscoveryProtocol<TestStore<ChaCha20Rng>, TestTopic, TestId, TestInfo> + 'static,
    {
        let (alice_tx, alice_rx) = mpsc::channel(16);
        let (bob_tx, bob_rx) = mpsc::channel(16);

        let bob_handle = tokio::task::spawn_local(async move {
            let Ok(result) = bob_protocol.bob(bob_tx, alice_rx).await else {
                panic!("running bob protocol failed");
            };
            result
        });

        let Ok(alice_result) = alice_protocol.alice(alice_tx, bob_rx).await else {
            panic!("running alice protocol failed");
        };

        for (_id, info) in alice_result.node_infos {
            self.store.insert_node_info(info).await.unwrap();
        }

        self.store
            .set_topics(remote.id, alice_result.node_topics)
            .await
            .expect("store failure");

        self.store
            .set_topic_ids(remote.id, alice_result.node_topic_ids)
            .await
            .expect("store failure");

        let bob_result = bob_handle.await.expect("local task failure");

        for (_id, info) in bob_result.node_infos {
            remote.store.insert_node_info(info).await.unwrap();
        }

        remote
            .store
            .set_topics(self.id, bob_result.node_topics)
            .await
            .expect("store failure");

        remote
            .store
            .set_topic_ids(self.id, bob_result.node_topic_ids)
            .await
            .expect("store failure");
    }
}

#[tokio::test]
async fn naive_protocol() {
    const NUM_NODES: usize = 10;
    const MAX_RUNS: usize = 10;

    let local = LocalSet::new();

    let handle = local.run_until(async move {
        // 1. Create test node instances.
        let nodes = {
            let mut result = HashMap::new();
            for id in 0..NUM_NODES {
                result.insert(id, Arc::new(RwLock::new(TestNode::new(id))));
            }
            result
        };

        // 2. Make every node aware of one bootstrap node, forming a transitive path across all nodes,
        //    so they have a chance to eventually learn about everyone.
        for my_id in 0..NUM_NODES - 1 {
            let my_node = nodes.get(&my_id).unwrap().read().await;
            my_node
                .store
                .insert_node_info(TestInfo {
                    id: my_id + 1,
                    bootstrap: true,
                    timestamp: 1,
                })
                .await
                .unwrap();
        }

        // 3. Launch every node in a separate task and run the discovery protocol for each of them.
        let mut set = JoinSet::new();

        for my_id in 0..NUM_NODES {
            let mut my_runs = 0;
            let nodes = nodes.clone();

            set.spawn_local(async move {
                loop {
                    if my_runs > MAX_RUNS {
                        break;
                    }

                    let my_node = nodes.get(&my_id).unwrap().read().await;
                    if let Some(id) = my_node.next_node().await {
                        let remote_node = nodes.get(&id).unwrap().read().await;

                        let alice_protocol = NaiveDiscoveryProtocol::new(
                            my_node.store.clone(),
                            my_node.subscription.clone(),
                            id,
                        );

                        let bob_protocol = NaiveDiscoveryProtocol::new(
                            remote_node.store.clone(),
                            remote_node.subscription.clone(),
                            my_id,
                        );

                        my_node
                            .connect(alice_protocol, bob_protocol, &remote_node)
                            .await;
                    }

                    my_runs += 1;
                }
            });
        }

        // Wait until all tasks have finished.
        set.join_all().await;

        // 4. Did every node discover all the others?
        for my_id in 0..NUM_NODES {
            let my_node = nodes.get(&my_id).unwrap().read().await;
            let all_infos = my_node.store.all_node_infos().await.unwrap();
            assert_eq!(all_infos.len(), NUM_NODES - 1);
        }
    });

    handle.await;
}
