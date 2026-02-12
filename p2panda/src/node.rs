// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_core::{Hash, PrivateKey, PublicKey};
use p2panda_net::NodeId;
use p2panda_net::gossip::GossipError;
use p2panda_store::sqlite::{SqliteError, SqliteStore, SqliteStoreBuilder};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::Topic;
pub use crate::builder::NodeBuilder;
use crate::network::{Network, NetworkConfig, NetworkError};
use crate::streams::{EphemeralStreamHandle, EventStream, StreamHandle};

// TODO: Can we expose network or will this explode the API surface for GObject unnecessarily?
pub struct Node {
    private_key: PrivateKey,
    public_key: PublicKey,
    store: SqliteStore<'static>,
    network: Network,
}

impl Node {
    pub fn builder() -> NodeBuilder {
        NodeBuilder::new()
    }

    pub async fn spawn() -> Result<Self, NodeError> {
        // Generates new private key using CSPRNG from system.
        let private_key = PrivateKey::new();

        // Initialises an in-memory SQLite database.
        let store = SqliteStoreBuilder::default().build().await?;

        // Use default config, this will _not_ include a bootstrap and relay and reduces the
        // functionality of p2panda to only work on local-area networks.
        let config = Config::default();

        Node::spawn_inner(config, private_key, store).await
    }

    pub(crate) async fn spawn_inner(
        config: Config,
        private_key: PrivateKey,
        store: SqliteStore<'static>,
    ) -> Result<Self, NodeError> {
        let public_key = private_key.public_key();
        let network = Network::spawn(config.network, private_key.clone()).await?;

        Ok(Node {
            private_key,
            public_key,
            store,
            network,
        })
    }

    pub async fn stream<M>(&self, _topic: Topic) -> Result<StreamHandle<M>, NodeError>
    where
        M: Serialize + for<'a> Deserialize<'a>,
    {
        unimplemented!()
    }

    pub async fn ephemeral_stream<M>(
        &self,
        topic: Topic,
    ) -> Result<EphemeralStreamHandle<M>, EphemeralStreamHandleError>
    where
        M: Serialize + for<'a> Deserialize<'a>,
    {
        let handle = self.network.gossip.stream(topic.into()).await?;

        Ok(EphemeralStreamHandle::new(
            topic,
            self.private_key.clone(),
            handle,
        ))
    }

    pub async fn events(&self) -> Result<EventStream, NodeError> {
        unimplemented!()
    }

    pub fn id(&self) -> NodeId {
        self.public_key
    }

    pub fn commit(&self, _message_id: Hash) {
        unimplemented!()
    }
}

/// Broken / closed communication channel with the internal gossip actor in `p2panda-net`. This can
/// be due to the actor crashing.
///
/// Users may re-attempt creating a new ephemeral stream handle in case the actor restarted later.
#[derive(Error, Debug)]
#[error("error occurred in internal gossip actor: {0}")]
pub struct EphemeralStreamHandleError(#[from] GossipError);

#[derive(Clone, Debug)]
pub struct Config {
    pub auto_commit: bool,
    pub network: NetworkConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            auto_commit: true,
            network: NetworkConfig::default(),
        }
    }
}

#[derive(Debug, Error)]
pub enum NodeError {
    #[error(transparent)]
    Network(#[from] NetworkError),

    #[error(transparent)]
    Store(#[from] SqliteError),
}
