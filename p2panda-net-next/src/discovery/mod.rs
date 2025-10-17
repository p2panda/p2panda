// SPDX-License-Identifier: MIT OR Apache-2.0

#![allow(unused)]
// @TODO: Make rng seedable for better testing.
use std::collections::{HashMap, HashSet};
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use futures_lite::future::{Boxed, BoxedLocal};
use iroh::RelayUrl;
use iroh::endpoint::DirectAddr;
use rand::seq::IteratorRandom;
use tokio::sync::{Mutex, RwLock, broadcast, mpsc};
use tokio::task::{JoinHandle, JoinSet};
use tokio::time;

use crate::NodeId;

#[derive(Clone)]
pub struct Discovery<T> {
    my_node_id: NodeId,
    endpoint: Endpoint<DiscoveryMessage>,
    address_book: Arc<RwLock<AddressBook<T>>>,
    config: Config,
    bootstrap_mode: Arc<Mutex<bool>>,
    sessions: Arc<RwLock<JoinSet<(NodeId, Result<DiscoveryResult<T>, DiscoveryError>)>>>,
    handle: Arc<JoinHandle<()>>,
    events: Arc<broadcast::Sender<DiscoveryEvent>>,
}

#[derive(Clone)]
pub struct Config {
    max_concurrent_sessions: usize,

    /// Value between 0-100 (percentage). Defaults to a 2% chance the random walk will reset to
    /// start with bootstrap nodes again.
    ///
    /// If 100 or higher the random walk will _always_ reset to bootstrap mode. If 0 it will
    /// _never_ reset and only do bootstrap mode once at the beginning.
    random_walk_reset_chance: usize,

    session_timeout: Duration,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            max_concurrent_sessions: 5,
            random_walk_reset_chance: 2,
            session_timeout: Duration::from_secs(60 * 2),
        }
    }
}

impl<T> Discovery<T>
where
    T: Topic + std::fmt::Debug + 'static,
{
    pub fn new(
        my_node_id: NodeId,
        endpoint: Endpoint<DiscoveryMessage>,
        config: Config,
    ) -> (Self, broadcast::Receiver<DiscoveryEvent>) {
        let (tx, rx) = broadcast::channel(16);

        let events = Arc::new(tx);
        let sessions = Arc::new(RwLock::new(JoinSet::new()));
        let address_book = Arc::new(RwLock::new(AddressBook::new()));

        let handle = {
            let events = events.clone();
            let sessions: Arc<
                RwLock<JoinSet<(NodeId, Result<DiscoveryResult<T>, DiscoveryError>)>>,
            > = sessions.clone();
            let address_book = address_book.clone();

            tokio::task::spawn_local(async move {
                loop {
                    let mut sessions = sessions.write().await;
                    let Some(result) = sessions.join_next().await else {
                        continue;
                    };

                    match result {
                        Ok((node_id, Ok(discovery_result))) => {
                            Self::on_success(
                                events.clone(),
                                address_book.clone(),
                                node_id,
                                discovery_result,
                            );
                        }
                        Ok((node_id, Err(discovery_err))) => {
                            Self::on_error(events.clone(), node_id, discovery_err);
                        }
                        Err(task_err) => {
                            // Task has been aborted, we're unwinding and finish this task here as
                            // well.
                            if task_err.is_cancelled() {
                                break;
                            }

                            // Critical failure in discovery session.
                            if task_err.is_panic() {
                                todo!();
                            }
                        }
                    }
                }
            })
        };

        (
            Self {
                my_node_id,
                address_book,
                endpoint,
                config,
                bootstrap_mode: Arc::new(Mutex::new(true)),
                sessions,
                handle: Arc::new(handle),
                events,
            },
            rx,
        )
    }

    async fn begin(&self) {
        {
            let mut bootstrap_mode = self.bootstrap_mode.lock().await;

            *bootstrap_mode = if *bootstrap_mode {
                true
            } else {
                rand::random_range(0..100) <= self.config.random_walk_reset_chance
            };
        }
    }

    async fn begin_session(&self) {
        let random_node = {
            let address_book = self.address_book.read().await;
            if *self.bootstrap_mode.lock().await {
                address_book.random_bootstrap_node()
            } else {
                address_book.random_node()
            }
        };

        let Some(random_node) = random_node else {
            return;
        };

        {
            let sessions_len = self.sessions.read().await.len();
            if sessions_len > self.config.max_concurrent_sessions {
                return;
            }
        }

        let (tx, rx) = self
            .endpoint
            .connect(self.my_node_id, random_node.node_id)
            .await;

        let session = discovery_session_alice::<T>(self.address_book.clone(), tx, rx);
        self.spawn_session(session, random_node.node_id).await;
    }

    async fn accept_session(
        &self,
        node_id: NodeId,
        tx: mpsc::Sender<DiscoveryMessage>,
        rx: mpsc::Receiver<DiscoveryMessage>,
    ) {
        {
            let sessions_len = self.sessions.read().await.len();
            if sessions_len > self.config.max_concurrent_sessions {
                return;
            }
        }

        let session = discovery_session_bob::<T>(self.address_book.clone(), tx, rx);
        self.spawn_session(session, node_id).await;
    }

    async fn spawn_session<F>(&self, session: F, node_id: NodeId)
    where
        F: Future<Output = Result<DiscoveryResult<T>, DiscoveryError>> + 'static,
    {
        let session_timeout = self.config.session_timeout;

        self.events
            .send(DiscoveryEvent::BeginDiscoverySession(node_id));

        let mut sessions = self.sessions.write().await;
        sessions.spawn_local(async move {
            tokio::select! {
                result = session => {
                    return (node_id, result);
                },
                _ = time::sleep(session_timeout) => {
                    return (node_id, Err(DiscoveryError::Timeout));
                }
            };
        });
    }

    async fn on_success(
        events: Arc<broadcast::Sender<DiscoveryEvent>>,
        address_book: Arc<RwLock<AddressBook<T>>>,
        remote_node_id: NodeId,
        result: DiscoveryResult<T>,
    ) {
        let mut address_book = address_book.write().await;
        for (node_id, node_info) in result.nodes {
            address_book.add_node(node_info);
        }

        events.send(DiscoveryEvent::EndDiscoverySession(remote_node_id));
    }

    async fn on_error(
        events: Arc<broadcast::Sender<DiscoveryEvent>>,
        remote_node_id: NodeId,
        error: DiscoveryError,
    ) {
    }

    pub async fn close(&mut self) {
        let mut sessions = self.sessions.write().await;
        sessions.shutdown().await;
        self.handle.abort();
    }
}

impl<T> ProtocolHandler<DiscoveryMessage> for Discovery<T>
where
    T: Topic + std::fmt::Debug + 'static,
{
    fn accept(
        &self,
        node_id: NodeId,
        tx: mpsc::Sender<DiscoveryMessage>,
        rx: mpsc::Receiver<DiscoveryMessage>,
    ) -> Pin<Box<dyn Future<Output = ()> + '_>> {
        Box::pin(async move {
            self.accept_session(node_id, tx, rx).await;
        })
    }
}

#[derive(Clone)]
pub enum DiscoveryMessage {}

async fn discovery_session_alice<T>(
    address_book: Arc<RwLock<AddressBook<T>>>,
    tx: mpsc::Sender<DiscoveryMessage>,
    rx: mpsc::Receiver<DiscoveryMessage>,
) -> Result<DiscoveryResult<T>, DiscoveryError> {
    // @TODO: Make sure to also send ourselves.
    Ok(DiscoveryResult::default())
}

async fn discovery_session_bob<T>(
    address_book: Arc<RwLock<AddressBook<T>>>,
    tx: mpsc::Sender<DiscoveryMessage>,
    rx: mpsc::Receiver<DiscoveryMessage>,
) -> Result<DiscoveryResult<T>, DiscoveryError> {
    // @TODO: Make sure to also send ourselves.
    Ok(DiscoveryResult::default())
}

#[derive(Clone, Debug)]
pub struct DiscoveryResult<T> {
    nodes: HashMap<NodeId, NodeInfo<T>>,
}

impl<T> Default for DiscoveryResult<T> {
    fn default() -> Self {
        Self {
            nodes: HashMap::new(),
        }
    }
}

#[derive(Clone, Debug)]
pub enum DiscoveryEvent {
    BeginDiscoverySession(NodeId),
    EndDiscoverySession(NodeId),
}

#[derive(Debug)]
pub enum DiscoveryError {
    Timeout,
}

// ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
// This might move to the address book
// ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

#[derive(Debug, Clone)]
pub struct NodeInfo<T> {
    /// Unique identifier of this node.
    pub node_id: NodeId,

    /// Prioritized node for "bootstrapping" the network.
    pub bootstrap: bool,

    /// Generic topics this node is "interested" in.
    pub topics: HashSet<T>,

    /// Possible endpoints this node can be connected via.
    pub endpoints: Vec<EndpointInfo>,
}

#[derive(Debug, Clone)]
pub enum EndpointInfo {
    Iroh {
        relay: RelayUrl,
        direct_addresses: Vec<DirectAddr>,
    },
}

// ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
// This is "dummy code" and will be removed
// ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

struct AddressBook<T> {
    nodes: HashMap<NodeId, NodeInfo<T>>,
}

impl<T> AddressBook<T>
where
    T: Topic,
{
    pub fn new() -> Self {
        Self {
            nodes: HashMap::default(),
        }
    }

    pub fn add_node(&mut self, node_info: NodeInfo<T>) {
        // @TODO: Merge node info instead of overwriting it.
        self.nodes.insert(node_info.node_id, node_info);
    }

    pub fn random_bootstrap_node(&self) -> Option<NodeInfo<T>> {
        self.nodes
            .values()
            .filter(|info| info.bootstrap)
            .cloned()
            .choose(&mut rand::rng())
    }

    pub fn random_node(&self) -> Option<NodeInfo<T>> {
        self.nodes.values().cloned().choose(&mut rand::rng())
    }
}

pub trait Topic: Clone {}

#[derive(Clone)]
pub struct Endpoint<T> {
    handlers: Arc<RwLock<HashMap<NodeId, Box<dyn ProtocolHandler<T>>>>>,
}

impl<T> Endpoint<T> {
    pub fn new() -> Self {
        Self {
            handlers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn register_protocol<P: ProtocolHandler<T> + 'static>(
        &self,
        node_id: NodeId,
        handler: P,
    ) {
        let mut handlers = self.handlers.write().await;
        handlers.insert(node_id, Box::new(handler));
    }

    pub async fn connect(
        &self,
        my_node_id: NodeId,
        remote_node_id: NodeId,
    ) -> (mpsc::Sender<T>, mpsc::Receiver<T>) {
        let (tx, rx) = mpsc::channel(16);
        let (remote_tx, remote_rx) = mpsc::channel(16);

        let mut handlers = self.handlers.read().await;
        handlers
            .get(&remote_node_id)
            .expect("node id is known")
            .accept(my_node_id, remote_tx, rx)
            .await;

        (tx, remote_rx)
    }
}

pub trait ProtocolHandler<T> {
    fn accept(
        &self,
        node_id: NodeId,
        tx: mpsc::Sender<T>,
        rx: mpsc::Receiver<T>,
    ) -> Pin<Box<dyn Future<Output = ()> + '_>>;
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::time::Duration;

    use p2panda_core::{PrivateKey, PublicKey};
    use tokio::task;

    use crate::NodeId;

    use super::{Config, Discovery, Endpoint, NodeInfo, Topic};

    #[derive(Clone, Debug)]
    enum MyTopic {
        Dogs,
        Cats,
    }

    impl Topic for MyTopic {}

    #[tokio::test]
    async fn it_works() {
        let local = task::LocalSet::new();

        local
            .run_until(async move {
                let endpoint = Endpoint::new();

                let node_id_1 = PrivateKey::new().public_key();
                let (node_1, node_1_rx) =
                    Discovery::<MyTopic>::new(node_id_1, endpoint.clone(), Config::default());

                endpoint.register_protocol(node_id_1, node_1.clone()).await;

                let node_id_2 = PrivateKey::new().public_key();
                let (node_2, mut node_2_rx) =
                    Discovery::<MyTopic>::new(node_id_2, endpoint.clone(), Config::default());

                {
                    let mut address_book_2 = node_2.address_book.write().await;
                    address_book_2.add_node(NodeInfo {
                        node_id: node_id_1,
                        bootstrap: true,
                        topics: HashSet::new(),
                        endpoints: Vec::new(),
                    });
                }

                endpoint.register_protocol(node_id_2, node_2.clone()).await;

                node_2.begin_session().await;

                let event = node_2_rx.recv().await;
                println!("{:?}", event);
                let event = node_2_rx.recv().await;
                println!("{:?}", event);

                tokio::time::sleep(Duration::from_millis(1)).await;
            })
            .await;
    }
}
