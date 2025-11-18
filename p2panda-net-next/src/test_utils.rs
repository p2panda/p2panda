// SPDX-License-Identifier: MIT OR Apache-2.0

use std::convert::Infallible;
use std::net::{Ipv4Addr, Ipv6Addr};
use std::pin::Pin;

use futures_channel::mpsc::{self, SendError};
use futures_util::Sink;
use p2panda_core::PrivateKey;
use p2panda_discovery::address_book::memory::MemoryStore;
use p2panda_sync::ToSync;
use p2panda_sync::traits::{Protocol, SyncManager};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha20Rng;

use crate::addrs::{NodeId, NodeInfo};
use crate::args::{ApplicationArguments, ArgsBuilder};
use crate::config::IrohConfig;
use crate::{NetworkId, TopicId};

pub const TEST_NETWORK_ID: NetworkId = [1; 32];

pub fn test_args() -> (
    ApplicationArguments,
    MemoryStore<ChaCha20Rng, NodeId, NodeInfo>,
    NoSyncConfig,
) {
    test_args_from_seed(rand::random())
}

pub fn test_args_from_seed(
    seed: [u8; 32],
) -> (
    ApplicationArguments,
    MemoryStore<ChaCha20Rng, NodeId, NodeInfo>,
    NoSyncConfig,
) {
    let mut rng = ChaCha20Rng::from_seed(seed);
    let store = MemoryStore::<ChaCha20Rng, NodeId, NodeInfo>::new(rng.clone());
    let private_key_bytes: [u8; 32] = rng.random();
    (
        ArgsBuilder::new(TEST_NETWORK_ID)
            .with_private_key(PrivateKey::from_bytes(&private_key_bytes))
            .with_iroh_config(IrohConfig {
                bind_ip_v4: Ipv4Addr::LOCALHOST,
                bind_port_v4: rng.random_range(49152..65535),
                bind_ip_v6: Ipv6Addr::LOCALHOST,
                bind_port_v6: rng.random_range(49152..65535),
                ..Default::default()
            })
            .with_rng(rng)
            .build(),
        store,
        NoSyncConfig,
    )
}

pub fn setup_logging() {
    if std::env::var("RUST_LOG").is_ok() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .try_init();
    }
}

#[test]
fn deterministic_args() {
    let (args_1, _, _) = test_args_from_seed([0; 32]);
    let (args_2, _, _) = test_args_from_seed([0; 32]);
    assert_eq!(args_1.public_key, args_2.public_key);
    assert_eq!(args_1.iroh_config, args_2.iroh_config);
}

pub struct NoProtocol;

impl Protocol for NoProtocol {
    type Output = ();
    type Error = Infallible;
    type Event = ();
    type Message = ();

    async fn run(
        self,
        _sink: &mut (impl Sink<Self::Message, Error = impl std::fmt::Debug> + Unpin),
        _stream: &mut (
                 impl futures_util::Stream<Item = Result<Self::Message, impl std::fmt::Debug>> + Unpin
             ),
    ) -> Result<Self::Output, Self::Error> {
        Ok(())
    }
}

#[derive(Clone, Debug, Default)]
pub struct NoSyncManager;

#[derive(Clone, Debug, Default)]
pub struct NoSyncConfig;

impl SyncManager<TopicId> for NoSyncManager {
    type Protocol = NoProtocol;
    type Config = NoSyncConfig;
    type Error = SendError;

    fn from_config(_config: Self::Config) -> Self {
        NoSyncManager
    }

    async fn session(
        &mut self,
        _session_id: u64,
        _config: &p2panda_sync::SyncSessionConfig<TopicId>,
    ) -> Self::Protocol {
        NoProtocol
    }

    fn session_handle(
        &self,
        _session_id: u64,
    ) -> Option<std::pin::Pin<Box<dyn Sink<ToSync, Error = Self::Error>>>> {
        let (tx, _) = mpsc::channel::<ToSync>(128);
        let sink = Box::pin(tx) as Pin<Box<dyn Sink<ToSync, Error = Self::Error>>>;
        Some(sink)
    }

    async fn next_event(
        &mut self,
    ) -> Result<Option<p2panda_sync::FromSync<()>>, Self::Error> {
        Ok(None)
    }
}
