// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fmt::Debug;

use futures_util::Stream;
use p2panda_core::{Hash, Topic};
use p2panda_net::iroh_endpoint::RelayUrl;
use p2panda_net::{NetworkId, NodeId};
use p2panda_spaces::manager::GLOBAL_GROUPS_CONTEXT_ID;
use p2panda_spaces::{GroupId, SpaceId, SpacesStoreState};
use p2panda_store::groups::GroupsStore;
use p2panda_store::spaces::{SpacesStore, SqliteSpacesStore};
use p2panda_store::sqlite::{SqliteError, SqliteStore, SqliteStoreBuilder};
use p2panda_store::topics::TopicStore;
use p2panda_store::{Transaction, tx};
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub use crate::builder::NodeBuilder;
use crate::credentials::Credentials;
use crate::forge::{Forge, OperationForge};
use crate::network::{Network, NetworkConfig, NetworkError};
use crate::operation::{Extensions, LogId};
use crate::processor::{Pipeline, TaskTracker};
use crate::spaces::types::{InnerSpace, NoBody, SpacesManager, SpacesManagerError};
use crate::spaces::{
    AccessLevel, ActorId, Group, GroupError, KEY_BUNDLE_LOG_ID, Member, MemberError, Space,
    SpaceError, SpaceSubscription, actor_to_topic, spaces_manager, spaces_stream,
    to_initial_members,
};
use crate::streams::{
    EphemeralStreamPublisher, EphemeralStreamSubscription, StreamFrom, StreamPublisher,
    StreamSubscription, SystemEvent, ephemeral_stream, event_stream, process_published_operation,
    processed_stream,
};

/// Node API with methods to establish ephemeral and eventually consistent topic streams.
#[derive(Debug)]
pub struct Node {
    config: Config,
    store: SqliteStore,
    forge: OperationForge,
    credentials: Credentials,
    // NOTE: One single pipeline is currently used to handle _all_ incoming operations, independent
    // of number of streams. While this is sufficient for most applications for now we might want to
    // make the number of processors configurable to avoid head-of-line blocking.
    pipeline: Pipeline<LogId, Extensions, Topic>,
    network: Network,
    spaces_manager: SpacesManager,
}

impl Node {
    /// Returns the builder for a `Node`.
    pub fn builder() -> NodeBuilder {
        NodeBuilder::new()
    }

    /// Spawns a `Node` using default configuration parameters.
    ///
    /// A [`SpawnError`] is returned if spawning is unsuccessful due to a network or store-related
    /// failure.
    pub async fn spawn() -> Result<Self, SpawnError> {
        // Initialises an in-memory SQLite database.
        let store = SqliteStoreBuilder::default().build().await?;

        // Generate random keys.
        let credentials = Credentials::generate();

        // Use default config, this will _not_ include a bootstrap and relay and reduces the
        // functionality of p2panda to only work on local-area networks.
        let config = Config::default();

        Node::spawn_inner(config, store, credentials).await
    }

    pub(crate) async fn spawn_inner(
        config: Config,
        store: SqliteStore,
        credentials: Credentials,
    ) -> Result<Self, SpawnError> {
        let forge = OperationForge::new(credentials.clone(), store.clone());

        let network = Network::spawn(
            config.network.clone(),
            credentials.node_signing_key(),
            store.clone(),
        )
        .await?;

        // TODO: Expose -spaces configuration to public API.
        let spaces_manager =
            spaces_manager(forge.clone(), credentials.clone(), store.clone()).await?;

        // Prepare manager which orchestrates processing of incoming operations.
        let tasks = TaskTracker::new();
        let pipeline = Pipeline::new(store.clone(), tasks, spaces_manager.clone());

        Ok(Node {
            config,
            store,
            forge,
            credentials,
            pipeline,
            network,
            spaces_manager,
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
    /// # let topic = Topic::random();
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

    /// Returns a publisher and subscriber pair for an ephemeral stream of messages over the given
    /// topic.
    ///
    /// Messages sent or received on this stream will not be persisted in local storage. Only
    /// currently online and reachable nodes will receive published messages.
    ///
    /// Message payloads are signed providing integrity and provenance guarantees, plus making sure
    /// each message is unique with the help of a timestamp.
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

        Ok(ephemeral_stream(topic, self.credentials.clone(), handle))
    }

    /// Returns a stream of system events.
    ///
    /// System events include all network-related events, such as discovery events, which are not
    /// associated with a specific topic.
    ///
    /// Any events generated before this method is called will _not_ be emitted. Therefore, it's
    /// recommended to call `event_stream()` right after the `Node` is spawned if you wish to
    /// observe network behaviour throughout the lifetime of the `Node`.
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

    pub async fn register_member(&self, member: Member) -> Result<(), MemberError> {
        let member: p2panda_spaces::member::Member = member.into();
        self.spaces_manager.register_member(&member).await?;

        Ok(())
    }

    pub async fn group(&self, group_id: impl Into<GroupId>) -> Result<Option<Group>, GroupError> {
        match self.spaces_manager.group(group_id.into()).await? {
            Some(inner) => {
                let topic = actor_to_topic(inner.id());
                let (tx, rx) = self.stream::<NoBody>(topic).await?;

                Ok(Some(Group::new(inner, tx, rx)))
            }
            None => Ok(None),
        }
    }

    pub async fn create_group(
        &self,
        initial_members: &[(ActorId, AccessLevel)],
    ) -> Result<Group, GroupError> {
        let initial_members = to_initial_members(initial_members);
        let (_, group_id, message) = self.spaces_manager.create_group(&initial_members).await?;

        // TODO: Could refactor this to process using the tx, similar like create_space. Like this
        // we would already receive the CREATED_GROUP event on the rx which is nice.
        let topic = actor_to_topic(group_id);
        let event =
            process_published_operation(message.into_operation(), topic, &self.pipeline).await;

        if event.is_failed() {
            // @TODO: we remove the first error here but there might be more which we should also
            // return to the user.
            Err(event.failure_reasons().remove(0))?
        } else {
            let group = self.group(group_id).await?.expect("");
            Ok(group)
        }
    }

    pub async fn space<M>(
        &self,
        space_id: impl Into<SpaceId>,
    ) -> Result<(Space<M>, SpaceSubscription<M>), SpaceError>
    where
        M: Serialize + for<'a> Deserialize<'a> + Send + 'static,
    {
        self.space_from(space_id, StreamFrom::Frontier).await
    }

    pub async fn space_from<M>(
        &self,
        space_id: impl Into<SpaceId>,
        from: StreamFrom,
    ) -> Result<(Space<M>, SpaceSubscription<M>), SpaceError>
    where
        M: Serialize + for<'a> Deserialize<'a> + Send + 'static,
    {
        let space_id = space_id.into();

        // Associate the space topic with the key bundle logs.
        //
        // This does _not_ happen during ingest as at that point there is no topic which could be
        // used to perform this association. The same key bundle log can be associated with many
        // spaces.
        tx!(&self.store, {
            self.store
                .associate(
                    &Topic::from(space_id),
                    &self.id(),
                    &Hash::digest(KEY_BUNDLE_LOG_ID),
                )
                .await
        })?;

        let topic = space_id;
        let inner = self
            .spaces_manager
            .space(space_id)
            .await?
            // @TODO: even if there is no space yet we allow the user to subscribe and get a
            // handle to the as-yet-non-existent space. In the current API if they tried to use
            // the space API _before_ the space is instantiated then an error would occur. We
            // maybe want to consider how we communicate to the user that they are subscribed to
            // the space topic but only to announce their key bundles and await receiving control
            // messages.
            .unwrap_or(InnerSpace::new(self.spaces_manager.clone(), space_id));
        let (tx, rx) = self.stream_from::<M>(topic, from).await?;

        // Publish one key bundle whenever we subscribe to a space.
        //
        // @TODO: this is a rather naive approach, we likely want some (configurable?) service
        // that periodically publishes key bundles.S
        let message = self.spaces_manager.key_bundle_message().await?;

        let operation = message.into_operation();
        let processed = tx
            .import(futures_util::stream::once(async { operation }))
            .await?;

        // Wait until processing the events has finished.

        // TODO: Would be good to get an error / report here if processing the imported operations
        // failed. This error so far only tells us that the channel broke down.
        if processed.await.is_err() {
            panic!();
        }

        Ok(spaces_stream::<M>(inner, self.store.clone(), tx, rx))
    }

    pub async fn create_space<M>(
        &self,
        space_id: impl Into<SpaceId>,
    ) -> Result<(Space<M>, SpaceSubscription<M>), SpaceError>
    where
        M: Serialize + for<'a> Deserialize<'a> + Send + 'static,
    {
        let space_id = space_id.into();

        // Associate the space topic with the key bundle logs.
        //
        // This does _not_ happen during ingest as at that point there is no topic which could be
        // used to perform this association. The same key bundle log can be associated with many
        // spaces.
        tx!(&self.store, {
            self.store
                .associate(
                    &Topic::from(space_id),
                    &self.id(),
                    &Hash::digest(KEY_BUNDLE_LOG_ID),
                )
                .await
        })?;

        // Establish a topic pub/sub stream using the space id as a topic. This also associates
        // the key bundle log with the space topic.
        let topic = space_id;
        let (tx, rx) = self.stream::<M>(topic).await?;

        // Publish one key bundle whenever we create a space.
        //
        // @TODO: this is a rather naive approach, we likely want some service that periodically
        // publishes key bundles.
        let message = self.spaces_manager.key_bundle_message().await?;

        let operation = message.into_operation();
        let processed = tx
            .import(futures_util::stream::once(async { operation }))
            .await?;

        // Wait until processing the events has finished.

        // TODO: Would be good to get an error / report here if processing the imported operations
        // failed. This error so far only tells us that the channel broke down.
        if processed.await.is_err() {
            panic!();
        }

        // Issue the event to create a space.
        //
        // We always create a space with only us as the initial members.
        //
        // @TODO: Consider if we want an alternative method for instantiating a space with initial
        // members. I (sam) removed it from the API for now as without a manual member
        // registration flow a user likely doesn't have access to any member key bundles at the
        // point of space creation.
        let (groups_y, space_y, messages) = self.spaces_manager.create_space(space_id, &[]).await?;

        // Persist the computed groups and spaces state to the stores and make required group log
        // associations.
        //
        // @TODO: This needs some thought. We're persisting state here, rather than expecting this
        // to happen in the processor, because it's not possible to re-create locally performed
        // state changes from the messages alone. _If_ we have to persist here, then transactions
        // need to be considered more carefully, we would need to make this write part of the same
        // transaction where the operations are forged.
        //
        // @TODO: We don't strictly need to persist the groups state here as this is re-creatable
        // with only the operation in the processor.
        let permit = self.store.begin().await?;

        let spaces_store = SqliteSpacesStore::<Extensions>::new(self.store.clone());
        spaces_store
            .set_groups_state_tx(Hash::digest(GLOBAL_GROUPS_CONTEXT_ID), &groups_y)
            .await?;
        spaces_store
            .set_space_state_tx(&space_id, &SpacesStoreState::from(space_y))
            .await?;

        self.store.commit(permit).await?;

        // Process the -spaces events by importing them as an "external stream".
        //
        // @TODO: Related to above comment. Even though the state is already mutated & persisted
        // locally we're still sending the messages to the pipeline. Revisit this if state does
        // indeed need to be persisted here instead of in the pipeline.
        let processed = tx
            .import(futures_util::stream::iter(
                messages.into_iter().map(|message| message.into_operation()),
            ))
            .await?;

        // Wait until processing the events has finished. This should result in a "materialised
        // space" we can finally call and return to the user.

        // TODO: Would be good to get an error / report here if processing the imported operations
        // failed. This error so far only tells us that the channel broke down.
        if processed.await.is_err() {
            panic!();
        }

        let inner = self
            .spaces_manager
            .space(space_id)
            .await?
            .expect("materialised space after processing operations");

        let (space, rx) = spaces_stream::<M>(inner, self.store.clone(), tx, rx);
        Ok((space, rx))
    }

    /// Returns the node identifier (public key).
    pub fn id(&self) -> NodeId {
        self.forge.verifying_key()
    }

    /// Returns the network identifier being used by the node.
    pub fn network_id(&self) -> NetworkId {
        self.network.network_id()
    }

    pub async fn me(&self) -> Result<Member, MemberError> {
        let inner = self.spaces_manager.me().await?;

        Ok(Member { inner })
    }

    /// Inserts a bootstrap node into the local address book.
    ///
    /// Bootstrap nodes are used as a starting point for the random-walk discovery algorithm to
    /// find other nodes in the network, without the need for any centralised registry. Any node
    /// can serve as a bootstrap into the network. The URL of the relay used by the bootstrap node
    /// is required to assist with connectivity (via relaying of traffic and negotiation of
    /// hole-punching for direct connections).
    ///
    /// Multiple bootstrap nodes can be registered. Each iteration of the discovery algorithm
    /// begins by picking a random node from the set of known bootstrap nodes. It's recommended to
    /// register several bootstrap nodes, especially if they are not highly-available; this
    /// offers redunancy in the case that any of the bootstrap nodes go offline or are otherwise
    /// unavailable.
    ///
    /// Consult the documentation of the `p2panda-discovery` crate for further details concerning
    /// the discovery protocol.
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
    /// Returns a clone of the underlying store for this `Node`.
    // NOTE(adz): This feels like something we would like to have on the regular Node API as well,
    // I'll leave it here for now until we've made a decision.
    pub fn store(&self) -> SqliteStore {
        self.store.clone()
    }

    /// Access the inner spaces manager.
    pub fn spaces_manager(&self) -> SpacesManager {
        self.spaces_manager.clone()
    }
}

/// Message acknowledgement policy for eventually-consistent topic streams.
///
/// Every `StreamSubscription` instance is stateful and keeps track of already acknowledged
/// operations by persisting them in the local SQLite database. Specifying a policy defines how and
/// when events are acknowledged.
///
/// Operations which have not been acknowledged yet will be automatically re-played when this stream
/// is created again.
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

/// Error occurred when spawning network or store processes.
#[derive(Debug, Error)]
#[allow(clippy::large_enum_variant)] // TODO: Reduce size of spaces error types.
pub enum SpawnError {
    #[error(transparent)]
    Network(#[from] NetworkError),

    #[error(transparent)]
    Store(#[from] SqliteError),

    #[error(transparent)]
    SpacesManager(#[from] SpacesManagerError),
}

/// Broken / closed communication channel with the internal actor in `p2panda-net` prevented
/// creation of stream. This can be due to the actor crashing.
///
/// Users may re-attempt creating a new stream in case the actor restarted later.
#[derive(Debug, Error)]
#[error("error occurred in internal actor: {0}")]
pub struct CreateStreamError(pub String);
