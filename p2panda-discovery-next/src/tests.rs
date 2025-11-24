// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashMap;
use std::sync::Arc;

use futures_channel::mpsc;
use futures_util::StreamExt;
use rand::SeedableRng;
use rand_chacha::ChaCha20Rng;
use tokio::sync::RwLock;
use tokio::task::{JoinSet, LocalSet};

use crate::DiscoveryResult;
use crate::address_book::{AddressBookStore};
use crate::psi_hash::{ PsiHashDiscoveryProtocol};
use crate::random_walk::RandomWalker;
use crate::test_utils::{TestId, TestInfo, TestStore, TestSubscription, TestTransportInfo};
use crate::traits::{DiscoveryProtocol, DiscoveryStrategy};

struct TestWalker {
    #[allow(unused)]
    id: usize,
    strategy: RandomWalker<ChaCha20Rng, TestStore<ChaCha20Rng>, TestId, TestInfo>,
    previous_result: Option<DiscoveryResult<TestId, TestInfo>>,
}

impl TestWalker {
    pub async fn next_node(&self) -> Option<TestId> {
        self.strategy
            .next_node(self.previous_result.as_ref())
            .await
            .unwrap()
    }
}

struct TestNode {
    id: TestId,
    subscription: TestSubscription,
    store: TestStore<ChaCha20Rng>,
    walkers: Vec<TestWalker>,
}

impl TestNode {
    pub fn new(id: TestId, walkers_num: usize, rng: ChaCha20Rng) -> Self {
        let store = TestStore::new(rng.clone());
        let subscription = TestSubscription::default();

        // Run multiple random-walkers at the same time.
        let walkers = {
            let mut result = Vec::new();
            for walker_id in 0..walkers_num {
                result.push(TestWalker {
                    id: walker_id,
                    strategy: RandomWalker::new(id, store.clone(), rng.clone()),
                    previous_result: None,
                });
            }
            result
        };

        Self {
            id,
            subscription,
            store,
            walkers,
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

    pub async fn connect<P>(
        &self,
        alice_protocol: P,
        bob_protocol: P,
        remote: &TestNode,
    ) -> DiscoveryResult<TestId, TestInfo>
    where
        P: DiscoveryProtocol<TestId, TestInfo> + 'static,
    {
        let (mut alice_tx, alice_rx) = mpsc::channel(16);
        let (mut bob_tx, bob_rx) = mpsc::channel(16);

        let bob_handle = tokio::task::spawn_local(async move {
            let mut alice_rx = alice_rx.map(|message| Ok::<_, ()>(message));
            let Ok(result) = bob_protocol.bob(&mut bob_tx, &mut alice_rx).await else {
                panic!("running bob protocol failed");
            };
            result
        });

        // Wait until Alice has finished and store their results
        let mut bob_rx = bob_rx.map(|message| Ok::<_, ()>(message));
        let Ok(alice_result) = alice_protocol.alice(&mut alice_tx, &mut bob_rx).await else {
            panic!("running alice protocol failed");
        };

        for (id, info) in alice_result.node_transport_infos {
            self.update_node_info(id, info).await;
        }

        self.store
            .set_sync_topics(remote.id, alice_result.sync_topics)
            .await
            .expect("store failure");

        self.store
            .set_ephemeral_messaging_topics(remote.id, alice_result.ephemeral_messaging_topics)
            .await
            .expect("store failure");

        // Wait until Bob has finished and store their results.
        let bob_result = bob_handle.await.expect("local task failure");

        for (id, info) in &bob_result.node_transport_infos {
            remote.update_node_info(*id, info.clone()).await;
        }

        remote
            .store
            .set_sync_topics(self.id, bob_result.sync_topics.clone())
            .await
            .expect("store failure");

        remote
            .store
            .set_ephemeral_messaging_topics(self.id, bob_result.ephemeral_messaging_topics.clone())
            .await
            .expect("store failure");

        // Return Bob's delivered info so Alice can continue with it.
        bob_result
    }
}

#[tokio::test]
async fn psi_hash_protocol() {
    const NUM_NODES: usize = 2;
    const NUM_WALKERS: usize = 1;
    const MAX_RUNS: usize = 1;

    let mut rng = ChaCha20Rng::from_seed([1; 32]);
    let local = LocalSet::new();

    let handle = local.run_until(async move {
        // 1. Create test node instances.
        let nodes = {
            let mut result = HashMap::new();
            for id in 0..NUM_NODES {
                result.insert(
                    id,
                    Arc::new(RwLock::new(TestNode::new(id, NUM_WALKERS, rng.clone()))),
                );
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
            for walker_id in 0..NUM_WALKERS {
                let nodes = nodes.clone();
                let mut walker_runs = 0;

                set.spawn_local(async move {
                    loop {
                        if walker_runs > MAX_RUNS {
                            break;
                        }

                        let my_node = nodes.get(&my_id).unwrap().read().await;
                        let walker = my_node.walkers.get(walker_id).unwrap();

                        if let Some(remote_id) = walker.next_node().await {
                            assert!(
                                remote_id != my_id,
                                "next_node should never return ourselves"
                            );

                            let remote_node = nodes.get(&remote_id).unwrap().read().await;

                            let alice_protocol = PsiHashDiscoveryProtocol::new(
                                my_node.store.clone(),
                                my_node.subscription.clone(),
                                remote_id,
                            );

                            let bob_protocol = PsiHashDiscoveryProtocol::new(
                                remote_node.store.clone(),
                                remote_node.subscription.clone(),
                                my_id,
                            );

                            my_node
                                .connect(alice_protocol, bob_protocol, &remote_node)
                                .await;
                        }

                        walker_runs += 1;
                    }
                });
            }
        }

        // Wait until all tasks have finished.
        set.join_all().await;

        // 4. Did every node discover all the others?
        for my_id in 0..NUM_NODES {
            let my_node = nodes.get(&my_id).unwrap().read().await;
            let all_nodes_len = my_node.store.all_nodes_len().await.unwrap();
            assert_eq!(all_nodes_len, NUM_NODES);
        }
    });

    handle.await;
}
