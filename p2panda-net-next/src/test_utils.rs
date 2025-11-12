// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_core::PrivateKey;
use p2panda_discovery::address_book::memory::MemoryStore;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha20Rng;

use crate::addrs::{NodeId, NodeInfo};
use crate::args::{ApplicationArguments, ArgsBuilder};
use crate::config::IrohConfig;
use crate::{NetworkId, TopicId};

pub const TEST_NETWORK_ID: NetworkId = [1; 32];

pub fn test_args() -> (
    ApplicationArguments,
    MemoryStore<ChaCha20Rng, TopicId, NodeId, NodeInfo>,
) {
    test_args_from_seed([1; 32])
}

pub fn test_args_from_seed(
    seed: [u8; 32],
) -> (
    ApplicationArguments,
    MemoryStore<ChaCha20Rng, TopicId, NodeId, NodeInfo>,
) {
    let mut rng = ChaCha20Rng::from_seed(seed);
    let store = MemoryStore::<ChaCha20Rng, TopicId, NodeId, NodeInfo>::new(rng.clone());
    let private_key_bytes: [u8; 32] = rng.random();
    (
        ArgsBuilder::new(TEST_NETWORK_ID)
            .with_private_key(PrivateKey::from_bytes(&private_key_bytes))
            .with_iroh_config(IrohConfig {
                bind_port_v4: rng.random_range(49152..65535),
                bind_port_v6: rng.random_range(49152..65535),
                ..Default::default()
            })
            .with_rng(rng)
            .build(),
        store,
    )
}

pub fn setup_logging() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
}

#[test]
fn deterministic_args() {
    let (args_1, _) = test_args();
    let (args_2, _) = test_args();
    assert_eq!(args_1.public_key, args_2.public_key);
    assert_eq!(args_1.iroh_config, args_2.iroh_config);
}
