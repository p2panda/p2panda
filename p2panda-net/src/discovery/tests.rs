// SPDX-License-Identifier: MIT OR Apache-2.0

use tokio::task::JoinHandle;

use crate::NodeId;
use crate::address_book::AddressBook;
use crate::addrs::NodeInfo;
use crate::discovery::{Discovery, DiscoveryEvent, SessionRole};
use crate::iroh_endpoint::Endpoint;
use crate::test_utils::{
    ApplicationArguments, generate_trusted_node_info, setup_logging, test_args_from_seed,
};

struct TestNode {
    args: ApplicationArguments,
    #[allow(unused)]
    endpoint: Endpoint,
    address_book: AddressBook,
    discovery: Discovery,
}

impl TestNode {
    pub async fn spawn(seed: [u8; 32], node_infos: Vec<NodeInfo>) -> Self {
        let (args, store) = test_args_from_seed(seed);

        let address_book = AddressBook::builder().store(store).spawn().await.unwrap();

        // Pre-populate the address book with known addresses.
        for info in node_infos {
            address_book.insert_node_info(info).await.unwrap();
        }

        let endpoint = Endpoint::builder(address_book.clone())
            .config(args.iroh_config.clone())
            .private_key(args.private_key.clone())
            .spawn()
            .await
            .unwrap();

        let discovery = Discovery::builder(address_book.clone(), endpoint.clone())
            .config(args.discovery_config.clone())
            .rng(args.rng.clone())
            .spawn()
            .await
            .unwrap();

        Self {
            args,
            address_book,
            endpoint,
            discovery,
        }
    }

    fn node_id(&self) -> NodeId {
        self.args.public_key
    }

    async fn session_ended_handle(&self) -> JoinHandle<()> {
        let mut events = self.discovery.events().await.unwrap();

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
}

#[tokio::test]
async fn smoke_test() {
    setup_logging();

    // Bob's address book is empty;
    let mut bob = TestNode::spawn([17; 32], vec![]).await;

    // Alice inserts Bob's info in their address book.
    let alice = TestNode::spawn([18; 32], vec![generate_trusted_node_info(&mut bob.args)]).await;

    // Wait until both parties finished at least one discovery session.
    let alice_session_ended = alice.session_ended_handle().await;
    let bob_session_ended = bob.session_ended_handle().await;
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
