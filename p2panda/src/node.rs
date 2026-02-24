// SPDX-License-Identifier: MIT OR Apache-2.0

use p2panda_core::{Hash, PrivateKey, PublicKey, Topic};
use p2panda_net::NodeId;
use p2panda_net::gossip::GossipError;
use p2panda_net::sync::LogSyncError;
use p2panda_store::sqlite::{SqliteError, SqliteStore, SqliteStoreBuilder};
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub use crate::builder::NodeBuilder;
use crate::network::{Network, NetworkConfig, NetworkError};
use crate::processor::Processor;
use crate::streams::{EphemeralStreamHandle, EventStream, StreamHandle};
use crate::{Extensions, Operation};

// TODO: Can we expose network or will this explode the API surface for GObject unnecessarily?
pub struct Node {
    private_key: PrivateKey,
    public_key: PublicKey,
    store: SqliteStore<'static>,
    // NOTE: One single processor is currently used to handle _all_ incoming operations,
    // independent of number of streams. While this is sufficient for most applications for now we
    // might want to make the number of processors configurable to avoid head-of-line blocking.
    processor: Processor<Topic, Extensions, Topic>,
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

        // Prepare manager which orchestrates processing of incoming operations.
        let processor = Processor::new::<SqliteStore<'static>>(store.clone());

        Node::spawn_inner(config, private_key, store, processor).await
    }

    pub(crate) async fn spawn_inner(
        config: Config,
        private_key: PrivateKey,
        store: SqliteStore<'static>,
        processor: Processor<Topic, Extensions, Topic>,
    ) -> Result<Self, NodeError> {
        let public_key = private_key.public_key();
        let network = Network::spawn(config.network, private_key.clone(), store.clone()).await?;

        Ok(Node {
            private_key,
            public_key,
            store,
            processor,
            network,
        })
    }

    pub async fn stream<M>(&self, topic: Topic) -> Result<StreamHandle<M>, StreamHandleError>
    where
        M: Clone + Serialize + for<'a> Deserialize<'a> + Send + 'static,
    {
        let handle = self
            .network
            .log_sync
            .stream(topic.into(), true)
            .await
            .map_err(|err| StreamHandleError(err.to_string()))?;

        StreamHandle::new(topic, handle, self.processor.clone())
            .await
            .map_err(|err| StreamHandleError(err.to_string()))
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

/// Broken / closed communication channel with the internal log sync actor in `p2panda-net`. This
/// can be due to the actor crashing.
///
/// Users may re-attempt creating a new stream handle in case the actor restarted later.
#[derive(Error, Debug)]
#[error("error occurred in internal log sync actor: {0}")]
pub struct StreamHandleError(String);

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
