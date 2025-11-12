// SPDX-License-Identifier: MIT OR Apache-2.0

use std::convert::Infallible;
use std::net::{Ipv4Addr, Ipv6Addr};
use std::pin::Pin;

use futures::SinkExt;
use futures::channel::mpsc::{self, SendError};
use futures_util::Sink;
use p2panda_core::PrivateKey;
use p2panda_discovery::address_book::memory::MemoryStore;
use p2panda_sync::ToSync;
use p2panda_sync::traits::{Protocol, SyncManager};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha20Rng;
use tracing::Level;

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
                bind_ip_v4: Ipv4Addr::LOCALHOST,
                bind_port_v4: rng.random_range(49152..65535),
                bind_ip_v6: Ipv6Addr::LOCALHOST,
                bind_port_v6: rng.random_range(49152..65535),
                ..Default::default()
            })
            .with_rng(rng)
            .build(),
        store,
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
    let (args_1, _) = test_args();
    let (args_2, _) = test_args();
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
        sink: &mut (impl Sink<Self::Message, Error = impl std::fmt::Debug> + Unpin),
        stream: &mut (
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

impl<T> SyncManager<T> for NoSyncManager {
    type Protocol = NoProtocol;
    type Config = NoSyncConfig;
    type Error = SendError;

    fn from_config(config: Self::Config) -> Self {
        NoSyncManager
    }

    fn session(
        &mut self,
        session_id: u64,
        config: &p2panda_sync::SyncSessionConfig<T>,
    ) -> Self::Protocol {
        NoProtocol
    }

    fn session_handle(
        &self,
        session_id: u64,
    ) -> Option<std::pin::Pin<Box<dyn Sink<ToSync, Error = Self::Error>>>> {
        let (tx, rx) = mpsc::channel::<ToSync>(128);
        let sink = Box::pin(tx) as Pin<Box<dyn Sink<ToSync, Error = Self::Error>>>;
        Some(sink)
    }

    async fn next_event(
        &mut self,
    ) -> Result<Option<p2panda_sync::SyncManagerEvent<T, ()>>, Self::Error> {
        Ok(None)
    }
}
