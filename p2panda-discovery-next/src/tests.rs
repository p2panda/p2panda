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
use crate::test_utils::{
    TestId, TestInfo, TestStore, TestSubscription, TestTopic, TestTransportInfo,
};
use crate::traits::{DiscoveryProtocol, DiscoveryStrategy};

struct TestNode {
    id: TestId,
    subscription: TestSubscription,
    store: TestStore<ChaCha20Rng>,
    strategy: RandomWalk<ChaCha20Rng, TestStore<ChaCha20Rng>, TestTopic, TestId, TestInfo>,
}

impl TestNode {
    pub fn new(id: TestId, rng: ChaCha20Rng) -> Self {
        let store = TestStore::new(rng.clone());

        let subscription = TestSubscription::default();
        let strategy = RandomWalk::new(id, store.clone(), rng);

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

    async fn update_node_info(&self, id: TestId, transports: TestTransportInfo) {
        // Retrieve existing node info or create a new one.
        let mut node_info = match self.store.node_info(&id).await.unwrap() {
            Some(info) => info,
            None => TestInfo::new(id),
        };

        // Update the attached transport info. If it is older than what we currently have it will
        // be simply ignored.
        if node_info.update_transports(transports) {
            self.store.insert_node_info(node_info).await.unwrap();
        }
    }

    pub async fn connect<P>(&self, alice_protocol: P, bob_protocol: P, remote: &TestNode)
    where
        P: DiscoveryProtocol<TestTopic, TestId, TestInfo> + 'static,
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

        for (id, info) in alice_result.node_transport_infos {
            self.update_node_info(id, info).await;
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

        for (id, info) in bob_result.node_transport_infos {
            remote.update_node_info(id, info).await;
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

    let mut rng = ChaCha20Rng::from_seed([1; 32]);
    let local = LocalSet::new();

    let handle = local.run_until(async move {
        // 1. Create test node instances.
        let nodes = {
            let mut result = HashMap::new();
            for id in 0..NUM_NODES {
                result.insert(id, Arc::new(RwLock::new(TestNode::new(id, rng.clone()))));
            }
            result
        };

        // 2. Make every node aware of one bootstrap node, forming a transitive path across all nodes,
        //    so they have a chance to eventually learn about everyone.
        for my_id in 0..NUM_NODES - 1 {
            let my_node = nodes.get(&my_id).unwrap().read().await;

            // Add ourselves to the address book.
            my_node
                .store
                .insert_node_info(TestInfo::new(my_id).with_random_address(&mut rng))
                .await
                .unwrap();

            // Add another bootstrap peer to the address book.
            my_node
                .store
                .insert_node_info({
                    TestInfo::new_bootstrap(my_id + 1).with_random_address(&mut rng)
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
                    if let Some(remote_id) = my_node.next_node().await {
                        assert!(
                            remote_id != my_id,
                            "next_node should never return ourselves"
                        );

                        let remote_node = nodes.get(&remote_id).unwrap().read().await;

                        let alice_protocol = NaiveDiscoveryProtocol::new(
                            my_node.store.clone(),
                            my_node.subscription.clone(),
                            remote_id,
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
            let all_infos_len = my_node.store.all_node_infos_len().await.unwrap();
            assert_eq!(all_infos_len, NUM_NODES);
        }
    });

    handle.await;
}
