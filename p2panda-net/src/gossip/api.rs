// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;

use futures_util::{Stream, StreamExt};
use p2panda_discovery::address_book::NodeInfo as _;
use ractor::{ActorRef, call};
use thiserror::Error;
use tokio::sync::{RwLock, broadcast, mpsc};
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::wrappers::errors::BroadcastStreamRecvError;
use tracing::trace;

use crate::address_book::{AddressBook, AddressBookError};
use crate::gossip::actors::ToGossipManager;
use crate::gossip::builder::Builder;
use crate::gossip::events::GossipEvent;
use crate::iroh_endpoint::Endpoint;
use crate::utils::ShortFormat;
use crate::{NodeId, TopicId};

/// Mapping of topic to the associated sender channels for getting messages into and out of the
/// gossip overlay.
type GossipSenders = HashMap<
    TopicId,
    (
        mpsc::Sender<Vec<u8>>,
        broadcast::Sender<Vec<u8>>,
        TopicDropGuard,
    ),
>;

/// Gossip protocol to broadcast ephemeral messages to all online nodes interested in the same
/// topic.
///
/// ## Example
///
/// ```rust
/// # use std::error::Error;
/// #
/// # #[tokio::main]
/// # async fn main() -> Result<(), Box<dyn Error>> {
/// # use futures_util::StreamExt;
/// # use p2panda_net::{AddressBook, Discovery, Endpoint, MdnsDiscovery, Gossip};
/// # let address_book = AddressBook::builder().spawn().await?;
/// # let endpoint = Endpoint::builder(address_book.clone())
/// #     .spawn()
/// #     .await?;
/// #
/// // Gossip uses the address book to watch for nodes interested in the same topic.
/// let gossip = Gossip::builder(address_book, endpoint).spawn().await?;
///
/// // Join overlay with given topic.
/// let handle = gossip.stream([1; 32]).await?;
///
/// // Publish a message.
/// handle.publish(b"Hello, Panda!").await?;
///
/// // Subscribe to messages.
/// let mut rx = handle.subscribe();
///
/// tokio::spawn(async move {
///     while let Some(Ok(_bytes)) = rx.next().await {
///         // ..
///     }
/// });
/// #
/// # Ok(())
/// # }
/// ```
///
/// ## Ephemeral Messaging
///
/// These unreliable “ephemeral” streams are intended to be used for relatively short-lived
/// messages without persistence and catch-up of past state, for example for "Awareness" or
/// "Presence" features. In most cases, messages will only be received if they were published after
/// the subscription was created.
///
/// Use [`LogSync`](crate::LogSync) if you wish to receive messages even after being offline for
/// guaranteed eventual consistency.
///
/// ## Self-healing overlay
///
/// Gossip-based broadcast overlays rely on membership protocols like [HyParView] which do not heal
/// from network fragmentation caused, for example, by bootstrap nodes going offline.
///
/// `p2panda-net` uses it's additional, confidential topic discovery layer in
/// [`Discovery`](crate::Discovery) to automatically heal these partitions. Whenever possible, it
/// allows nodes a higher chance to connect to every participating node, thereby decentralising the
/// entrance points into the network.
///
/// ## Topic Discovery
///
/// For gossip to function correctly we need to inform it about discovered nodes who are interested
/// in the same topic. Check out the [`Discovery`](crate::Discovery) module for more information.
///
/// [HyParView]: https://asc.di.fct.unl.pt/~jleitao/pdf/dsn07-leitao.pdf
///
/// ## Leaving overlays on Drop
///
/// We're staying connected to the overlay for a topic as long as there's a `GossipHandle` or
/// `GossipSubscription` left for it. If all references to this topic are dropped, the gossip
/// overlay will be automatically left.
///
/// If all `Gossip` instances are dropped then the associated handles and subscriptions will fail
/// sending or receiving messages.
#[derive(Clone)]
pub struct Gossip {
    my_node_id: NodeId,
    address_book: AddressBook,
    inner: Arc<RwLock<Inner>>,
    senders: Arc<RwLock<GossipSenders>>,
}

struct Inner {
    actor_ref: ActorRef<ToGossipManager>,
}

impl Gossip {
    pub(crate) fn new(
        actor_ref: ActorRef<ToGossipManager>,
        my_node_id: NodeId,
        address_book: AddressBook,
    ) -> Self {
        Self {
            my_node_id,
            address_book,
            inner: Arc::new(RwLock::new(Inner { actor_ref })),
            senders: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn builder(address_book: AddressBook, endpoint: Endpoint) -> Builder {
        Builder::new(address_book, endpoint)
    }

    /// Join gossip overlay for this topic and return a handle to publish messages to it or receive
    /// messages from the network.
    pub async fn stream(&self, topic: TopicId) -> Result<GossipHandle, GossipError> {
        // Check if there's already a handle for this topic and clone it.
        //
        // If this handle exists but the topic counter is zero we know that all previous handles
        // have been dropped and we didn't clean up yet. In this case we'll ignore the existing
        // entry in "senders" and continue to create a new gossip session, overwriting the "dead"
        // entries.
        if let Some((to_gossip_tx, from_gossip_tx, guard)) = self.senders.read().await.get(&topic)
            && guard.has_subscriptions()
        {
            return Ok(GossipHandle::new(
                topic,
                to_gossip_tx.clone(),
                from_gossip_tx.clone(),
                guard.clone(),
            ));
        }

        // If there's no active handle for this topic we join the overlay from scratch.
        let inner = self.inner.read().await;

        // This guard counts the number of active handles and subscriptions for this topic. Like
        // this we can determine if we can leave the overlay.
        let guard = TopicDropGuard::new(topic, inner.actor_ref.clone());

        // Identify the initial nodes we can use to bootstrap ourselves into the overlay.
        let node_ids = {
            let node_infos = self.address_book.node_infos_by_topics([topic]).await?;
            node_infos
                .iter()
                .filter_map(|info| {
                    // Remove ourselves from list.
                    let node_id = info.id();
                    if node_id != self.my_node_id {
                        Some(node_id)
                    } else {
                        None
                    }
                })
                .collect()
        };

        // Register a new session with the gossip actor.
        let (to_gossip_tx, from_gossip_tx) =
            call!(inner.actor_ref, ToGossipManager::Subscribe, topic, node_ids)
                .map_err(Box::new)?;

        // Store the gossip senders.
        //
        // `from_gossip_tx` is used to create a broadcast receiver when the user calls
        // `subscribe()` on `GossipHandle`.
        let mut senders = self.senders.write().await;
        senders.insert(
            topic,
            (
                to_gossip_tx.clone(),
                from_gossip_tx.clone(),
                guard.clone_without_increment(),
            ),
        );

        Ok(GossipHandle::new(
            topic,
            to_gossip_tx,
            from_gossip_tx,
            guard,
        ))
    }

    /// Subscribe to system events.
    ///
    /// NOTE: only events emitted _after_ calling this method will be received on the returned
    /// channel.
    pub async fn events(&self) -> Result<broadcast::Receiver<GossipEvent>, GossipError> {
        let inner = self.inner.read().await;
        let result = call!(inner.actor_ref, ToGossipManager::Events).map_err(Box::new)?;
        Ok(result)
    }
}

impl Drop for Inner {
    fn drop(&mut self) {
        trace!(
            actor_id = %self.actor_ref.get_id(),
            "drop gossip actor reference"
        );

        // Stop actor after all references (Gossip, GossipHandle, GossipSubscription) have dropped.
        self.actor_ref.stop(None);
    }
}

#[derive(Debug, Error)]
pub enum GossipError {
    /// Spawning the internal actor failed.
    #[error(transparent)]
    ActorSpawn(#[from] ractor::SpawnErr),

    /// Messaging with internal actor via RPC failed.
    #[error(transparent)]
    ActorRpc(#[from] Box<ractor::RactorErr<ToGossipManager>>),

    #[error(transparent)]
    AddressBook(#[from] AddressBookError),
}

/// Handle for publishing ephemeral messages into the gossip overlay and receiving from the
/// network for a specific topic.
#[derive(Clone)]
pub struct GossipHandle {
    topic: TopicId,
    to_topic_tx: mpsc::Sender<Vec<u8>>,
    from_gossip_tx: broadcast::Sender<Vec<u8>>,
    _guard: TopicDropGuard,
}

impl GossipHandle {
    fn new(
        topic: TopicId,
        to_topic_tx: mpsc::Sender<Vec<u8>>,
        from_gossip_tx: broadcast::Sender<Vec<u8>>,
        _guard: TopicDropGuard,
    ) -> Self {
        Self {
            topic,
            to_topic_tx,
            from_gossip_tx,
            _guard,
        }
    }

    /// Publishes a message to the stream.
    pub async fn publish(
        &self,
        bytes: impl Into<Vec<u8>>,
    ) -> Result<(), mpsc::error::SendError<Vec<u8>>> {
        self.to_topic_tx.send(bytes.into()).await
    }

    /// Subscribes to the stream.
    ///
    /// The returned [`GossipSubscription`] provides a means of receiving messages from the
    /// stream.
    pub fn subscribe(&self) -> GossipSubscription {
        GossipSubscription::new(
            self.topic,
            self.from_gossip_tx.subscribe(),
            self._guard.clone(),
        )
    }

    /// Returns the topic of the stream.
    pub fn topic(&self) -> TopicId {
        self.topic
    }
}

/// A handle to an ephemeral messaging stream subscription.
///
/// The stream can be used to receive messages from the stream.
#[derive(Debug)]
pub struct GossipSubscription {
    topic: TopicId,
    from_topic_rx: BroadcastStream<Vec<u8>>,
    _guard: TopicDropGuard,
}

impl GossipSubscription {
    /// Returns a handle to an ephemeral messaging stream subscriber.
    fn new(
        topic: TopicId,
        from_topic_rx: broadcast::Receiver<Vec<u8>>,
        _guard: TopicDropGuard,
    ) -> Self {
        Self {
            topic,
            from_topic_rx: BroadcastStream::new(from_topic_rx),
            _guard,
        }
    }

    /// Returns the topic of the stream.
    pub fn topic(&self) -> TopicId {
        self.topic
    }
}

impl Stream for GossipSubscription {
    type Item = Result<Vec<u8>, BroadcastStreamRecvError>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        self.from_topic_rx.poll_next_unpin(cx)
    }
}

/// Helper maintaining a counter of references using the same topic.
///
/// Check if we can unsubscribe from topic if all handles and subscriptions have been dropped for
/// it. The gossip overlay will be left then for this topic.
#[derive(Debug)]
struct TopicDropGuard {
    topic: TopicId,
    counter: Arc<AtomicUsize>,
    actor_ref: ActorRef<ToGossipManager>,
    ignore_drop: bool,
}

/// Initial value the reference counter starts with.
///
/// This is set to a non-zero value since the first reference exists already when creating the
/// gossip stream.
const INITIAL_COUNTER: usize = 1;

impl TopicDropGuard {
    fn new(topic: TopicId, actor_ref: ActorRef<ToGossipManager>) -> Self {
        trace!(
            topic = topic.fmt_short(),
            counter = INITIAL_COUNTER,
            actor_id = %actor_ref.get_id(),
            "new topic drop guard"
        );

        Self {
            topic,
            counter: Arc::new(AtomicUsize::new(INITIAL_COUNTER)),
            actor_ref,
            ignore_drop: false,
        }
    }

    /// Returns current number of references to this topic.
    fn counter(&self) -> usize {
        self.counter.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Returns true if there's still one or more references for this topic used.
    fn has_subscriptions(&self) -> bool {
        self.counter() >= INITIAL_COUNTER
    }

    /// Clone guard, but don't increment reference counter.
    ///
    /// This is useful if we need to keep it around somewhere for further use without affecting the
    /// drop logic.
    fn clone_without_increment(&self) -> Self {
        Self {
            topic: self.topic,
            counter: self.counter.clone(),
            actor_ref: self.actor_ref.clone(),
            ignore_drop: true,
        }
    }
}

impl Clone for TopicDropGuard {
    fn clone(&self) -> Self {
        let value = self
            .counter
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);

        trace!(
            topic = self.topic.fmt_short(),
            counter = value + 1,
            actor_id = %self.actor_ref.get_id(),
            "clone topic drop guard +1"
        );

        Self {
            topic: self.topic,
            counter: self.counter.clone(),
            actor_ref: self.actor_ref.clone(),
            ignore_drop: false,
        }
    }
}

impl Drop for TopicDropGuard {
    fn drop(&mut self) {
        // This instance is not used to count references, we drop it without taking any action.
        if self.ignore_drop {
            return;
        }

        // Check if we can unsubscribe from topic if all handles and subscriptions have been
        // dropped for it.
        let previous_counter = self
            .counter
            .fetch_sub(1, std::sync::atomic::Ordering::SeqCst);

        trace!(
            topic = self.topic.fmt_short(),
            counter = previous_counter - 1,
            actor_id = %self.actor_ref.get_id(),
            "drop topic drop guard -1"
        );

        // If the previous value is equal the initial value, the last instance of the guard was
        // dropped and the counter has no references to the topic anymore.
        let no_references_left = previous_counter == INITIAL_COUNTER;

        if no_references_left {
            trace!(
                topic = self.topic.fmt_short(),
                actor_id = %self.actor_ref.get_id(),
                "send unsubscribe message"
            );

            // Ignore this error, it could be that the actor has already stopped.
            let _ = self
                .actor_ref
                .send_message(ToGossipManager::Unsubscribe(self.topic));
        }
    }
}

#[cfg(test)]
mod tests {
    use ractor::thread_local::{ThreadLocalActor, ThreadLocalActorSpawner};

    use crate::gossip::GossipConfig;
    use crate::gossip::actors::GossipManager;
    use crate::{AddressBook, Endpoint};

    use super::TopicDropGuard;

    #[tokio::test]
    async fn topic_drop_guard() {
        // A bit cumbersone, but that's the only way right now we get this actor ref from ractor.
        let (actor_ref, _) = {
            let address_book = AddressBook::builder().spawn().await.unwrap();
            let endpoint = Endpoint::builder(address_book.clone())
                .spawn()
                .await
                .unwrap();
            let thread_pool = ThreadLocalActorSpawner::new();
            let args = (GossipConfig::default(), address_book, endpoint);
            GossipManager::spawn(None, args, thread_pool).await.unwrap()
        };

        let guard_1 = TopicDropGuard::new([1; 32], actor_ref);
        assert_eq!(guard_1.counter(), 1);
        let _guard_2 = guard_1.clone();
        assert_eq!(guard_1.counter(), 2);
        let guard_3 = guard_1.clone();
        assert_eq!(guard_1.counter(), 3);

        drop(guard_3);
        assert_eq!(guard_1.counter(), 2);
    }
}
