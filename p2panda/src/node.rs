// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fmt::Debug;

use futures_util::Stream;
pub use p2panda_core::identity::{PrivateKey, PublicKey};
pub use p2panda_core::{Hash, Topic};
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
    EphemeralStreamPublisher, EphemeralStreamSubscription, StreamFrom, StreamPublisher,
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

    /// Returns a publisher and stateful subscriber for an eventually consistent event stream of
    /// messages over the given topic.
    ///
    /// This API is inspired by the principles of "event streaming", combined with eventually
    /// consistent "local-first" and causally ordered events.
    ///
    /// ## Event types
    ///
    /// Items emitted from the stream include application messages (delivered on top of p2panda's
    /// "operation" append-only log data-type), error and system events, for example about the sync
    /// session taking place on the networking layer.
    ///
    /// ## Event processing
    ///
    /// Every operation running through the subscription stream gets processed by an internal "event
    /// processing pipeline". This concerns the system-layer, meaning the internal p2panda
    /// append-only log `Operation` data-type and internal processors to derive state from these
    /// operations. Here we check the log-integrity, prune the log on demand, order operations
    /// causally and more.
    ///
    /// After this we're forwarding the message to the application-layer, with a bunch of meta data
    /// and debugging info attached.
    ///
    /// Applications usually want to further process the received events from the stream, for
    /// example validating the application specific message format to then finally change the state.
    ///
    /// These application messages can be deltas of CRDTs (Conflict-Free Replicated Data-Types) or
    /// concrete events, such as "move pawn to E4" in a chess-game. Usually applying these state
    /// transitions will lead to a new "materialization" of the application's state which is
    /// persisted in the app's database.
    ///
    /// This streaming API has a *at least once* guarantee, meaning that events can occur more than
    /// once. Any processing system needs to have an idempotency guarantee or account for tracking
    /// processed events.
    ///
    /// Events are automatically acknowledged by default and re-played when not acked on app-start,
    /// read further below for more details on the stateful design of stream subscribers, cursors
    /// and acknowledgments.
    ///
    /// ```text
    ///               ┌────────────────────────────────────────────┐
    ///               │                                            │   APPLICATION
    ///               │               User interface               │   (example)
    ///               │                                            │
    ///               └───▲────────────────────────────────────┬───┘
    ///                   │                                    ▼
    ///           ┌───────┼───────┐                       User Action
    ///           │               │                            │
    ///           │   Database    │                            │
    ///           │               │                            │
    ///           └───────────▲───┘                            │
    ///                       │                                │
    ///      Acknowledge      │                                │
    ///     ┌─────────┐       │                                │
    ///     │         │       │                                │
    ///     │     ┌───┼───────┼───┐                            │
    ///     │     │               │                            │
    ///     │     │  Application  │                            │
    ///     │     │  Stream       │                            │ Command
    ///     │     │  Processing   │                      ┌─────▼──────┐
    ///     │     │               │                      │Create Event│
    ///     │     │               │                      └─────┬──────┘
    ///     │     └───────▲───────┘                            │
    ///     │             │                                    │
    ///     │             │                                    │
    ///     │             │ rx                                 │ tx
    ///     │             │                                    │
    /// ────┼─────────────┼────────────────────────────────────┼──────────────────
    ///     │             │                                    │            SYSTEM
    ///     │     ┌───────┼───────┐                            │
    ///     │     │               │                            │ Publish
    ///     │     │               │           ┌────────────────▼─────────────────┐
    ///     │     │   System      │           │Create & sign p2panda operation w.│
    ///     │     │   Stream      │           │"message" payload from application│
    ///     │     │   Processing  │           └────────────────┬─────────────────┘
    ///     │     │               │                            │
    /// ┌───▼───┐ │               │                            │
    /// │ Acked │ │               │                            │
    /// │ State │ │               │                            │
    /// └───┬───┘ │               │                            │
    ///     └─────┤               │                            │
    ///           └───────▲───────┘                            │
    ///                   │                                    │
    ///                   │                                    │
    ///                   │◄───────────────────────────────────┤
    ///                   │                                    │
    ///                   │                                    │
    ///                   │ Receive from other nodes           │ Publish
    ///                   │                                    │
    ///               ┌───┼────────────────────────────────────▼──┐
    ///               │                                           │
    ///               │                p2p network                │
    ///               │                                           │
    ///               └───────────────────────────────────────────┘
    /// ```
    ///
    /// Locally created operations (via the stream publisher) are processed by the same pipeline. It
    /// is possible to await the processing result which can be useful for some applications if they
    /// want to block UI components etc.
    ///
    /// ```rust
    /// # use p2panda_core::Topic;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let topic = Topic::new();
    /// # let node = p2panda::builder().spawn().await?;
    /// #
    /// let (tx, _) = node.stream::<String>(topic).await?;
    ///
    /// // Publish a message, internally this creates an "operation" which needs to be processed.
    /// let processing = tx.publish("I'm being processed soon!".into()).await?;
    ///
    /// // The hash of the created operation is directly available.
    /// let hash = processing.hash();
    ///
    /// // We can optionally await the result of the processor.
    /// let result = processing.await?;
    /// assert!(result.is_completed());
    /// assert!(!result.is_failed());
    /// #
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// ## Stateful subscriptions and acknowledgments
    ///
    /// The returned [`StreamSubscription`] is stateful and keeps track of already acknowledged
    /// operations by persisting them in the local SQLite database. Operations which have not been
    /// acknowledged yet will be automatically re-played when this stream is created again.
    ///
    /// By default all events are automatically acknowledged. Use [`AckPolicy`] to change this
    /// behaviour when configuring the node. It is recommended to switch to a manual policy and
    /// explicitly acknowledge events _after_ processing them on application-layer was successful
    /// (see diagram above). Like this applications can ensure every event is at least processed
    /// once, guaranteeing resiliance in the context of application crashes.
    ///
    /// The topic is used to identify each stream's state. It is not recommended to create more than
    /// one subscription over the same topic using this high-level method as the acked state will be
    /// shared across them, leading to potentially surprising behaviour ("work stealing" processing
    /// behaviour across streams and potentially more duplicate events).
    ///
    /// Applications _never_ acknowledge events which only concern system-level state (for example
    /// pruning events without a payload, key agreement "control messages" etc.), these are _always_
    /// acknowledged automatically after they've been processed successfully, independent of the
    /// chosen ack policy.
    ///
    /// ## Crash Resiliance & Re-plays
    ///
    /// Un-acknowledged ("nacked") events are automatically re-played when a stream is created by
    /// default. This gives us the "at least once" guarantee, making sure no events get lost, even
    /// when facing system crashes or other unexpected exits (for example a user moving a mobile
    /// application into the background, interrupting all current processing).
    ///
    /// With the [`Node::stream_from`] method we can further determine the behaviour of re-plays.
    /// For example we can begin streaming from a custom "cursor" position on or request to stream
    /// _all_ currently known events for this topic from the start. All of these tools allow for
    /// different patterns of application state materialization, rolling out breaking changes,
    /// updates, etc.
    ///
    /// Please note that this can be a destructive action as it will _replace_ and persist the
    /// current acked stream state with the new arguments.
    ///
    /// ## System-level failures
    ///
    /// In most cases application developers will not need to deal with the system-level event
    /// processing part. However, in rare cases (bugs, critical failures, etc.) processing an event,
    /// re-playing or acknowledging it might have failed.
    ///
    /// Usually these situations are connected to system failure (running out of resource like
    /// hard-disc space) or bugs in p2panda. Since failed system-level events are not acknowledged,
    /// they will be automatically replayed when the application starts again. If the underlying
    /// cause of the error was not fixed by that, then you might want to consult if any patches have
    /// been made in p2panda.
    pub async fn stream<M>(
        &self,
        topic: impl Into<Topic>,
    ) -> Result<(StreamPublisher<M>, StreamSubscription<M>), CreateStreamError>
    where
        M: Serialize + for<'a> Deserialize<'a> + Send + 'static,
    {
        self.stream_from(topic, StreamFrom::Frontier).await
    }

    /// Eventually consistent publish and subscribe stream of messages from a given position.
    ///
    /// Use [`StreamFrom`] to determine the starting position of the subscription stream.
    ///
    /// See [`Node::stream`] for further information.
    pub async fn stream_from<M>(
        &self,
        topic: impl Into<Topic>,
        from: StreamFrom,
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
            from,
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

    pub async fn insert_bootstrap(
        &self,
        node_id: NodeId,
        relay_url: RelayUrl,
    ) -> Result<(), NetworkError> {
        self.network.insert_bootstrap(node_id, relay_url).await
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
