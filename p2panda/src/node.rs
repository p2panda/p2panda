// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_core::{Hash, Topic};
use p2panda_net::NodeId;
use p2panda_net::gossip::GossipError;
use p2panda_net::sync::LogSyncError;
use p2panda_store::sqlite::{SqliteError, SqliteStore, SqliteStoreBuilder};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::Extensions;
pub use crate::builder::NodeBuilder;
use crate::forge::{Forge, OperationForge};
use crate::network::{Network, NetworkConfig, NetworkError};
use crate::streams::{EphemeralStreamHandle, EventStream, StreamHandle};

// TODO: Can we expose network or will this explode the API surface for GObject unnecessarily?
pub struct Node {
    store: SqliteStore<'static>,
    forge: OperationForge,
    network: Network,
}

impl Node {
    pub fn builder() -> NodeBuilder {
        NodeBuilder::new()
    }

    pub async fn spawn() -> Result<Self, NodeError> {
        // Initialises an in-memory SQLite database.
        let store = SqliteStoreBuilder::default().build().await?;

        // Create a forge with a new internally-generated private key.
        let forge = OperationForge::new(store.clone());

        // Use default config, this will _not_ include a bootstrap and relay and reduces the
        // functionality of p2panda to only work on local-area networks.
        let config = Config::default();

        Node::spawn_inner(config, store, forge).await
    }

    pub(crate) async fn spawn_inner(
        config: Config,
        store: SqliteStore<'static>,
        forge: OperationForge,
    ) -> Result<Self, NodeError> {
        let network =
            Network::spawn(config.network, forge.private_key().clone(), store.clone()).await?;

        Ok(Node {
            store,
            forge,
            network,
        })
    }

    pub async fn stream<M>(&self, topic: Topic) -> Result<StreamHandle<M>, LogSyncError<Extensions>>
    where
        M: Serialize + for<'a> Deserialize<'a>,
    {
        let handle = self.network.log_sync.stream(topic.into(), true).await?;
        let forge = self.forge.clone();

        Ok(StreamHandle::new(topic, handle, forge))
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
            self.forge.private_key().clone(),
            handle,
        ))
    }

    pub async fn events(&self) -> Result<EventStream, NodeError> {
        unimplemented!()
    }

    pub fn id(&self) -> NodeId {
        self.forge.public_key()
    }

    pub fn ack(&self, _message_id: Hash) {
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

#[derive(Clone, Default, Debug)]
pub enum AckPolicy {
    /// Each individual message must be acknowledged.
    Explicit,

    /// No manual acknowledgment needed, node assumes acknowledgment on delivery.
    #[default]
    Automatic,
}

#[derive(Clone, Default, Debug)]
pub(crate) struct Config {
    pub ack_policy: AckPolicy,
    pub network: NetworkConfig,
}

#[derive(Debug, Error)]
pub enum NodeError {
    #[error(transparent)]
    Network(#[from] NetworkError),

    #[error(transparent)]
    Store(#[from] SqliteError),
}
