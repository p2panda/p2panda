// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fmt::Debug;

use futures_util::Stream;
use p2panda_core::{Hash, Topic};
pub use p2panda_net::iroh_endpoint::{EndpointAddr, RelayUrl};
pub use p2panda_net::iroh_mdns::MdnsDiscoveryMode;
pub use p2panda_net::{NetworkId, NodeId};
use p2panda_store::sqlite::{SqliteError, SqliteStore, SqliteStoreBuilder};
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub use crate::builder::NodeBuilder;
use crate::forge::{Forge, OperationForge};
use crate::network::{Network, NetworkConfig, NetworkError};
use crate::operation::{Extensions, LogId};
use crate::processor::{Pipeline, TaskTracker};
use crate::streams::{
    EphemeralStreamPublisher, EphemeralStreamSubscription, Offset, StreamPublisher,
    StreamSubscription, SystemEvent, ephemeral_stream, event_stream, processed_stream,
};

#[derive(Debug)]
pub struct Node {
    config: Config,
    #[allow(unused)]
    store: SqliteStore,
    forge: OperationForge,
    // NOTE: One single pipeline is currently used to handle _all_ incoming operations,
    // independent of number of streams. While this is sufficient for most applications for now we
    // might want to make the number of processors configurable to avoid head-of-line blocking.
    pipeline: Pipeline<LogId, Extensions, Topic>,
    network: Network,
}

impl Node {
    pub fn builder() -> NodeBuilder {
        NodeBuilder::new()
    }

    pub async fn spawn() -> Result<Self, SpawnError> {
        // Initialises an in-memory SQLite database.
        let store = SqliteStoreBuilder::default().build().await?;

        // Create a forge with a new internally-generated private key.
        let forge = OperationForge::new(store.clone());

        // Use default config, this will _not_ include a bootstrap and relay and reduces the
        // functionality of p2panda to only work on local-area networks.
        let config = Config::default();

        // Prepare manager which orchestrates processing of incoming operations.
        let tasks = TaskTracker::new();
        let pipeline = Pipeline::new::<SqliteStore>(store.clone(), tasks);

        let node = Node::spawn_inner(config, store, forge, pipeline).await?;

        Ok(node)
    }

    pub(crate) async fn spawn_inner(
        config: Config,
        store: SqliteStore,
        forge: OperationForge,
        pipeline: Pipeline<LogId, Extensions, Topic>,
    ) -> Result<Self, NetworkError> {
        let network = Network::spawn(
            config.network.clone(),
            forge.private_key().clone(),
            store.clone(),
        )
        .await?;

        Ok(Node {
            config,
            store,
            forge,
            pipeline,
            network,
        })
    }

    /// Eventually consistent publish and subscribe stream of messages.
    ///
    /// Items emitted from the stream include operations, sync events and system events (ie.
    /// network-related events, such as discovery events, which are not directly associated with a
    /// specific topic).
    pub async fn stream<M>(
        &self,
        topic: impl Into<Topic>,
    ) -> Result<(StreamPublisher<M>, StreamSubscription<M>), CreateStreamError>
    where
        M: Serialize + for<'a> Deserialize<'a> + Send + 'static,
    {
        self.stream_from(topic, Offset::Frontier).await
    }

    /// Eventually consistent publish and subscribe stream of messages with a custom offset.
    ///
    /// Setting an offset is useful if the application doesn't keep any materialised state around
    /// and needs to repeat all messages on start.
    ///
    /// Another use-case is the roll-out of an application update where all state needs to be
    /// re-materialised.
    pub async fn stream_from<M>(
        &self,
        topic: impl Into<Topic>,
        offset: Offset,
    ) -> Result<(StreamPublisher<M>, StreamSubscription<M>), CreateStreamError>
    where
        M: Serialize + for<'a> Deserialize<'a> + Send + 'static,
    {
        let live_mode = true;
        let topic = topic.into();

        let sync_handle = self
            .network
            .log_sync
            .stream(topic, live_mode)
            .await
            .map_err(|err| CreateStreamError(err.to_string()))?;

        let (tx, rx) = processed_stream(
            topic,
            self.config.ack_policy,
            sync_handle,
            self.store.clone(),
            self.forge.clone(),
            self.pipeline.clone(),
            offset,
        )
        .await
        .map_err(|err| CreateStreamError(err.to_string()))?;

        Ok((tx, rx))
    }

    pub async fn ephemeral_stream<M>(
        &self,
        topic: impl Into<Topic>,
    ) -> Result<(EphemeralStreamPublisher<M>, EphemeralStreamSubscription<M>), CreateStreamError>
    where
        M: Serialize + for<'a> Deserialize<'a>,
    {
        let topic = topic.into();
        let handle = self
            .network
            .gossip
            .stream(topic)
            .await
            .map_err(|err| CreateStreamError(err.to_string()))?;

        Ok(ephemeral_stream(topic, self.forge.clone(), handle))
    }

    /// System event stream.
    ///
    /// System events include all network-related events, such as discovery events, which are not
    /// associated with a specific topic.
    pub async fn event_stream(
        &self,
    ) -> Result<impl Stream<Item = SystemEvent> + Send + Unpin + 'static, CreateStreamError> {
        let discovery_events = self
            .network
            .discovery
            .events()
            .await
            .map_err(|err| CreateStreamError(err.to_string()))?;

        Ok(event_stream(discovery_events))
    }

    pub fn id(&self) -> NodeId {
        self.forge.public_key()
    }

    pub fn ack(&self, _message_id: Hash) {
        unimplemented!()
    }
}

#[cfg(any(test, feature = "test_utils"))]
impl Node {
    // NOTE(adz): This feels like something we would like to have on the regular Node API as well,
    // I'll leave it here for now until we've made a decision.
    pub fn store(&self) -> SqliteStore {
        self.store.clone()
    }
}

#[derive(Clone, Copy, Default, Debug, PartialEq)]
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
pub enum SpawnError {
    #[error(transparent)]
    Network(#[from] NetworkError),

    #[error(transparent)]
    Store(#[from] SqliteError),
}

/// Broken / closed communication channel with the internal actor in `p2panda-net` prevented
/// creation of stream. This can be due to the actor crashing.
///
/// Users may re-attempt creating a new stream in case the actor restarted later.
#[derive(Error, Debug)]
#[error("error occurred in internal actor: {0}")]
pub struct CreateStreamError(String);
