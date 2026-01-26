// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use futures_util::StreamExt;
use p2panda_core::PrivateKey;
use p2panda_discovery::address_book::memory::MemoryStore;
use p2panda_net::addrs::NodeInfo;
use p2panda_net::iroh_mdns::MdnsDiscoveryMode;
use p2panda_net::test_utils::{
    ApplicationArguments, TestClient, TestLogId, TestNode, setup_logging, test_args_from_seed,
};
use p2panda_net::{NodeId, TopicId};
use p2panda_store::{LogStore, OperationStore};
use p2panda_sync::protocols::TopicLogSyncEvent;
use rand::Rng;
use rand_chacha::ChaCha20Rng;
use tokio::time::sleep;

const LOG_ID: TestLogId = 1;

const TOPIC: TopicId = [1; 32];

type AddressBookStore = MemoryStore<ChaCha20Rng, NodeId, NodeInfo>;

struct LocalSwarm {
    clients: HashMap<u8, (ApplicationArguments, AddressBookStore, TestClient)>,
    online_nodes: HashMap<u8, TestNode>,
}

impl LocalSwarm {
    pub fn new(max_size: u8) -> Self {
        let clients = {
            let mut result = HashMap::new();
            for id in 0..max_size {
                result.insert(id, Self::create_client(id));
            }
            result
        };

        Self {
            clients,
            online_nodes: HashMap::with_capacity(u8::MAX as usize),
        }
    }

    pub fn create_client(id: u8) -> (ApplicationArguments, AddressBookStore, TestClient) {
        let (mut args, store) = test_args_from_seed([id; 32]);
        args.mdns_mode = MdnsDiscoveryMode::Active;

        let client = TestClient::new(PrivateKey::from_bytes(&args.rng.random::<[u8; 32]>()));

        (args, store, client)
    }

    #[allow(unused)]
    pub fn is_online(&self, id: &u8) -> bool {
        self.online_nodes.contains_key(id)
    }

    pub async fn set_online(&mut self, id: u8) {
        let Some((args, store, client)) = self.clients.get(&id) else {
            panic!("unknown id");
        };

        let node =
            TestNode::spawn_with_args_and_client((args.clone(), store.clone()), client.clone())
                .await;
        let handle = node.log_sync.stream(TOPIC, true).await.unwrap();
        println!("start sync for {}", id);

        let mut store = node.client.store.clone();

        tokio::task::spawn(async move {
            let mut rx = handle.subscribe().await.unwrap();

            while let Some(Ok(from_sync)) = rx.next().await {
                match from_sync.event {
                    TopicLogSyncEvent::Operation(operation) => {
                        let hash = operation.hash;
                        let header_bytes = operation.header.to_bytes();
                        let header = operation.header;
                        let body = operation.body;

                        store
                            .insert_operation(hash, &header, body.as_ref(), &header_bytes, &LOG_ID)
                            .await
                            .unwrap();

                        println!("{} received operation", id);
                    }
                    TopicLogSyncEvent::LiveModeStarted => {
                        println!("{} joined overlay", id);
                    }
                    TopicLogSyncEvent::LiveModeFinished(_) => {
                        println!("{} left overlay", id);
                    }
                    TopicLogSyncEvent::Failed { error } => {
                        println!("{} sync failed with {}", id, error);
                    }
                    _ => (),
                }
            }

            println!("end sync for {}", id);
        });

        self.online_nodes.insert(id, node);
    }

    #[allow(unused)]
    pub fn set_offline(&mut self, id: u8) -> bool {
        self.online_nodes.remove(&id).is_some()
    }

    pub async fn write_message(&mut self, id: u8, bytes: &[u8]) {
        let Some((_, _, client)) = self.clients.get_mut(&id) else {
            panic!("unknown id");
        };

        client.create_operation(bytes, LOG_ID).await;

        client
            .insert_topic(&TOPIC, HashMap::from([(client.id(), vec![LOG_ID])]))
            .await;

        println!("{} writes message {:?}", id, bytes);
    }

    pub async fn messages_by_id(&self, id: u8) -> HashSet<Vec<u8>> {
        let Some((_, _, client)) = self.clients.get(&id) else {
            panic!("unknown id");
        };

        let mut result = HashSet::new();

        for (_, (_, _, sender_client)) in &self.clients {
            let operations = client
                .store
                .get_log(&sender_client.id(), &LOG_ID, None)
                .await
                .unwrap()
                .unwrap_or_default();

            let operations: HashSet<Vec<u8>> = operations
                .iter()
                .map(|(_header, body)| {
                    body.as_ref()
                        .expect("body is always set in tests")
                        .to_bytes()
                })
                .collect();

            result.extend(operations.into_iter());
        }

        result
    }
}

#[tokio::test]
async fn large_network() {
    setup_logging();

    const NODES_NUM: u8 = 8;

    let mut swarm = LocalSwarm::new(NODES_NUM);

    for id in 0..NODES_NUM {
        swarm.set_online(id).await;
    }

    for id in 0..NODES_NUM {
        swarm.write_message(id, &[id.to_be()]).await;
    }

    sleep(Duration::from_secs(60 * 1)).await;

    print!("   ");
    for sender_id in 0..NODES_NUM {
        print!("{} ", sender_id)
    }
    print!("\n");

    for receiver_id in 0..NODES_NUM {
        print!("{}: ", receiver_id);

        let messages = swarm.messages_by_id(receiver_id).await;
        for sender_id in 0..NODES_NUM {
            let result = messages.contains(&[sender_id.to_be()].to_vec());
            print!("{} ", if result { "x" } else { " " });

            // if !messages.contains(&[sender_id.to_be()].to_vec()) {
            //     panic!(
            //         "{} misses message from {} (has {} total)",
            //         receiver_id,
            //         sender_id,
            //         messages.len()
            //     );
            // }
        }
        print!("\n");
    }
}
