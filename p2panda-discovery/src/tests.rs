// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use futures_channel::mpsc;
use futures_util::StreamExt;
use p2panda_core::PrivateKey;
use p2panda_store::address_book::AddressBookStore;
use p2panda_store::address_book::test_utils::{TestNodeId, TestNodeInfo, TestTransportInfo};
use p2panda_store::{SqliteStore, tx_unwrap};
use rand::SeedableRng;
use rand_chacha::ChaCha20Rng;
use tokio::sync::RwLock;
use tokio::task::{JoinSet, LocalSet};

use crate::DiscoveryResult;
use crate::psi_hash::PsiHashDiscoveryProtocol;
use crate::random_walk::RandomWalker;
use crate::test_utils::TestSubscription;
use crate::traits::{DiscoveryProtocol, DiscoveryStrategy};

struct TestWalker {
    #[allow(unused)]
    id: usize,
    strategy: RandomWalker<ChaCha20Rng, SqliteStore<'static>, TestNodeId, TestNodeInfo>,
    previous_result: Option<DiscoveryResult<TestNodeId, TestNodeInfo>>,
}

impl TestWalker {
    pub async fn next_node(&self) -> Option<TestNodeId> {
        self.strategy
            .next_node(self.previous_result.as_ref())
            .await
            .unwrap()
    }
}

struct TestNode {
    id: TestNodeId,
    subscription: TestSubscription,
    store: SqliteStore<'static>,
    walkers: Vec<TestWalker>,
}

impl TestNode {
    pub async fn new(id: TestNodeId, walkers_num: usize, rng: ChaCha20Rng) -> Self {
        let store = SqliteStore::temporary().await;

        let mut subscription = TestSubscription::default();
        subscription.topics.insert([7; 32]);

        tx_unwrap!(store, {
            <SqliteStore<'_> as AddressBookStore<TestNodeId, TestNodeInfo>>::set_topics(
                &store,
                id,
                HashSet::from_iter([[7; 32]]),
            )
            .await
            .unwrap();
        });

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

    async fn update_node_info(&self, id: TestNodeId, transports: TestTransportInfo) {
        // Retrieve existing node info or create a new one.
        let mut node_info = match self.store.node_info(&id).await.unwrap() {
            Some(info) => info,
            None => TestNodeInfo::new(id),
        };

        // Update the attached transport info. If it is older than what we currently have it will
        // be simply ignored.
        if node_info.update_transports(transports) {
            tx_unwrap!(self.store, {
                self.store.insert_node_info(node_info).await.unwrap();
            });
        }
    }

    pub async fn connect<P>(
        &self,
        alice_protocol: P,
        bob_protocol: P,
        remote: &TestNode,
    ) -> DiscoveryResult<TestNodeId, TestNodeInfo>
    where
        P: DiscoveryProtocol<TestNodeId, TestNodeInfo> + 'static,
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

        for (id, info) in alice_result.transport_infos {
            self.update_node_info(id, info).await;
        }

        tx_unwrap!(self.store, {
            <SqliteStore<'_> as AddressBookStore<TestNodeId, TestNodeInfo>>::set_topics(
                &self.store,
                remote.id,
                alice_result.topics,
            )
            .await
            .expect("store failure");
        });

        // Wait until Bob has finished and store their results.
        let bob_result = bob_handle.await.expect("local task failure");

        for (id, info) in &bob_result.transport_infos {
            remote.update_node_info(*id, info.clone()).await;
        }

        tx_unwrap!(remote.store, {
            <SqliteStore<'_> as AddressBookStore<TestNodeId, TestNodeInfo>>::set_topics(
                &remote.store,
                self.id,
                bob_result.topics.clone(),
            )
            .await
            .expect("store failure");
        });

        // Return Bob's delivered info so Alice can continue with it.
        bob_result
    }
}

#[tokio::test]
async fn peer_discovery_in_network() {
    const NUM_NODES: usize = 10;
    const NUM_WALKERS: usize = 4;
    const MAX_RUNS: usize = 10;

    let mut rng = ChaCha20Rng::from_seed([1; 32]);
    let local = LocalSet::new();

    let handle = local.run_until(async move {
        // 1. Create test node instances.
        let mut nodes_id_map = HashMap::new();

        let nodes = {
            let mut result = HashMap::new();
            for idx in 0..NUM_NODES {
                let id = PrivateKey::new().public_key();

                nodes_id_map.insert(id, idx);

                result.insert(
                    idx,
                    Arc::new(RwLock::new(
                        TestNode::new(id, NUM_WALKERS, rng.clone()).await,
                    )),
                );
            }
            result
        };

        // 2. Make every node aware of one bootstrap node, forming a transitive path across all nodes,
        //    so they have a chance to eventually learn about everyone.
        for my_idx in 0..NUM_NODES - 1 {
            let my_node = nodes.get(&my_idx).unwrap().read().await;
            let next_node = nodes.get(&(my_idx + 1)).unwrap().read().await;

            // Add ourselves to the address book.
            tx_unwrap!(my_node.store, {
                my_node
                    .store
                    .insert_node_info(TestNodeInfo::new(my_node.id).with_random_address(&mut rng))
                    .await
                    .unwrap();

                // Add another bootstrap peer to the address book.
                my_node
                    .store
                    .insert_node_info({
                        TestNodeInfo::new_bootstrap(next_node.id).with_random_address(&mut rng)
                    })
                    .await
                    .unwrap();
            });
        }

        // 3. Launch every node in a separate task and run the discovery protocol for each of them.
        let mut set = JoinSet::new();

        for my_idx in 0..NUM_NODES {
            for walker_id in 0..NUM_WALKERS {
                let nodes = nodes.clone();
                let nodes_id_map = nodes_id_map.clone();
                let mut walker_runs = 0;

                set.spawn_local(async move {
                    loop {
                        if walker_runs > MAX_RUNS {
                            break;
                        }

                        let my_node = nodes.get(&my_idx).unwrap().read().await;
                        let walker = my_node.walkers.get(walker_id).unwrap();

                        if let Some(remote_id) = walker.next_node().await {
                            assert!(
                                remote_id != my_node.id,
                                "next_node should never return ourselves"
                            );

                            let remote_idx = nodes_id_map.get(&remote_id).unwrap();
                            let remote_node = nodes.get(remote_idx).unwrap().read().await;

                            let alice_protocol = PsiHashDiscoveryProtocol::new(
                                my_node.store.clone(),
                                my_node.subscription.clone(),
                                my_node.id,
                                remote_id,
                            );

                            let bob_protocol = PsiHashDiscoveryProtocol::new(
                                remote_node.store.clone(),
                                remote_node.subscription.clone(),
                                remote_node.id,
                                my_node.id,
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
        for my_idx in 0..NUM_NODES {
            let my_node = nodes.get(&my_idx).unwrap().read().await;

            let all_nodes_len =
                <SqliteStore<'_> as AddressBookStore<TestNodeId, TestNodeInfo>>::all_nodes_len(
                    &my_node.store,
                )
                .await
                .expect("store failure");

            assert_eq!(all_nodes_len, NUM_NODES);
        }
    });

    handle.await;
}
