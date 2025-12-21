// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_core::PrivateKey;
use p2panda_discovery::address_book::memory::MemoryStore;
use rand::{Rng, SeedableRng};

use crate::events::{NetworkEvent, RelayStatus};
use crate::test_utils::{DummySyncConfig, DummySyncManager, setup_logging};
use crate::{Network, NetworkBuilder};

// TODO: Replace this with `iroh::test_utils::run_relay_server` when we removed the old p2panda-net
// and p2panda-blobs, as we can't satisfy the cargo dependencies with iroh's test-utils feature
// flag enabled.
const RELAY_URL: &str = "https://euc1-1.relay.n0.iroh-canary.iroh.link.";

#[tokio::test]
async fn learns_about_home_relay() {
    setup_logging();

    let mut rng = rand_chacha::ChaCha20Rng::from_seed([1; 32]);
    let address_book = MemoryStore::new(rng.clone());
    let private_key = PrivateKey::from_bytes(&{
        let bytes: [u8; 32] = rng.random();
        bytes
    });
    let (sync_config, _) = DummySyncConfig::new();

    let network: Network<DummySyncManager> = NetworkBuilder::new([1; 32])
        .relay(RELAY_URL.parse().unwrap())
        .private_key(private_key.clone())
        .build(address_book.clone(), sync_config.clone())
        .await
        .unwrap();

    let mut events = network.events().await.unwrap();
    loop {
        if let NetworkEvent::Relay(RelayStatus::Connected(_)) = events.recv().await.unwrap() {
            break;
        }
    }

    // Drop the whole network instance and test if we're coming back online with a relay connection
    // after launching a new instance with the same node id / private key.
    //
    // Read more here: https://github.com/n0-computer/iroh/issues/3798
    drop(network);

    let network: Network<DummySyncManager> = NetworkBuilder::new([1; 32])
        .relay(RELAY_URL.parse().unwrap())
        .private_key(private_key)
        .build(address_book, sync_config)
        .await
        .unwrap();

    let mut events = network.events().await.unwrap();
    loop {
        if let NetworkEvent::Relay(RelayStatus::Connected(_)) = events.recv().await.unwrap() {
            break;
        }
    }
}
