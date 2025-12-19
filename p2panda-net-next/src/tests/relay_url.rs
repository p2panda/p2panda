// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_discovery::address_book::memory::MemoryStore;
use rand::SeedableRng;

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

    let address_book = MemoryStore::new(rand_chacha::ChaCha20Rng::from_os_rng());
    let (sync_config, _) = DummySyncConfig::new();

    let network: Network<DummySyncManager> = NetworkBuilder::new([1; 32])
        .relay(RELAY_URL.parse().unwrap())
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
