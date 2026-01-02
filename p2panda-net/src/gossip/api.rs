// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::AtomicIsize;

use futures_util::{Stream, StreamExt};
use p2panda_discovery::address_book::NodeInfo as _;
use ractor::{ActorRef, call};
use thiserror::Error;
use tokio::sync::{RwLock, broadcast, mpsc};
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::wrappers::errors::BroadcastStreamRecvError;

use crate::address_book::{AddressBook, AddressBookError};
use crate::gossip::actors::ToGossipManager;
use crate::gossip::builder::Builder;
use crate::gossip::events::GossipEvent;
use crate::iroh_endpoint::Endpoint;
use crate::{NodeId, TopicId};

/// Mapping of topic to the associated sender channels for getting messages into and out of the
/// gossip overlay.
type GossipSenders = HashMap<TopicId, (mpsc::Sender<Vec<u8>>, broadcast::Sender<Vec<u8>>, Counter)>;

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

    pub async fn stream(&self, topic: TopicId) -> Result<GossipHandle, GossipError> {
        // Check if there's already a handle for this topic and clone it.
        //
        // If this handle exists but the topic counter is zero we know that all previous handles
        // have been dropped and we didn't clean up yet. In this case we'll ignore the existing
        // entry in "senders" and continue to create a new gossip session, overwriting the "dead"
        // entries.
        if let Some((to_gossip_tx, from_gossip_tx, guard)) = self.senders.read().await.get(&topic)
            && guard
                .topic_counter
                .load(std::sync::atomic::Ordering::SeqCst)
                > 0
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
        let guard = Counter {
            topic,
            // Since the counter increments by 1 on each clone and we don't want to count cloning
            // the guard into "senders", we start at -1 here.
            topic_counter: Arc::new(AtomicIsize::new(-1)),
            actor_ref: inner.actor_ref.clone(),
        };

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
            (to_gossip_tx.clone(), from_gossip_tx.clone(), guard.clone()),
        );

        Ok(GossipHandle::new(
            topic,
            to_gossip_tx,
            from_gossip_tx,
            guard,
        ))
    }

    /// Subscribe to system events.
    pub async fn events(&self) -> Result<broadcast::Receiver<GossipEvent>, GossipError> {
        let inner = self.inner.read().await;
        let result = call!(inner.actor_ref, ToGossipManager::Events).map_err(Box::new)?;
        Ok(result)
    }
}

impl Drop for Inner {
    fn drop(&mut self) {
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

/// Ephemeral streams provide an interface for publishing messages into the network and receiving
/// messages from the network.
///
/// Ephemeral streams are intended to be used for relatively short-lived messages without
/// persistence and catch-up of past state. In most cases, messages will only be received if they
/// were published after the subscription was created. The exception to this is if the message was
/// still propagating through the network at the time of the subscription; then it's possible that
/// the message is received, even though the publication time was strictly before that of the local
/// subscription event.
///
/// Use the eventually consistent stream if you wish to receive past state and (optionally)
/// messages representing the latest updates in an ongoing manner.
pub struct GossipHandle {
    topic: TopicId,
    to_topic_tx: mpsc::Sender<Vec<u8>>,
    from_gossip_tx: broadcast::Sender<Vec<u8>>,
    guard: Counter,
}

impl GossipHandle {
    fn new(
        topic: TopicId,
        to_topic_tx: mpsc::Sender<Vec<u8>>,
        from_gossip_tx: broadcast::Sender<Vec<u8>>,
        guard: Counter,
    ) -> Self {
        Self {
            topic,
            to_topic_tx,
            from_gossip_tx,
            guard,
        }
    }

    /// Publishes a message to the stream.
    pub async fn publish(&self, bytes: impl Into<Vec<u8>>) -> Result<(), GossipHandleError> {
        self.to_topic_tx
            .send(bytes.into())
            .await
            .map_err(Box::new)?;
        Ok(())
    }

    /// Subscribes to the stream.
    ///
    /// The returned `GossipSubscription` provides a means of receiving messages from the
    /// stream.
    pub fn subscribe(&self) -> GossipSubscription {
        GossipSubscription::new(
            self.topic,
            self.from_gossip_tx.subscribe(),
            self.guard.clone(),
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
pub struct GossipSubscription {
    topic: TopicId,
    from_topic_rx: BroadcastStream<Vec<u8>>,
    _guard: Counter,
}

impl GossipSubscription {
    /// Returns a handle to an ephemeral messaging stream subscriber.
    fn new(topic: TopicId, from_topic_rx: broadcast::Receiver<Vec<u8>>, _guard: Counter) -> Self {
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
    type Item = Result<Vec<u8>, GossipHandleError>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        self.from_topic_rx
            .poll_next_unpin(cx)
            .map_err(GossipHandleError::from)
    }
}

#[derive(Debug, Error)]
pub enum GossipHandleError {
    #[error(transparent)]
    Publish(#[from] Box<mpsc::error::SendError<Vec<u8>>>),

    #[error(transparent)]
    Subscribe(#[from] BroadcastStreamRecvError),
}

struct Counter {
    topic: TopicId,
    topic_counter: Arc<AtomicIsize>,
    actor_ref: ActorRef<ToGossipManager>,
}

impl Clone for Counter {
    fn clone(&self) -> Self {
        self.topic_counter
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);

        Self {
            topic: self.topic,
            topic_counter: self.topic_counter.clone(),
            actor_ref: self.actor_ref.clone(),
        }
    }
}

impl Drop for Counter {
    fn drop(&mut self) {
        let actor_ref = self.actor_ref.clone();

        // Check if we can unsubscribe from topic if all handles and subscriptions have been
        // dropped for it.
        let previous_counter = self
            .topic_counter
            .fetch_sub(1, std::sync::atomic::Ordering::SeqCst);

        // If this is 1 the last instance of the guard was dropped and the counter is now at zero.
        if previous_counter == 1 {
            // Ignore this error, it could be that the actor has already stopped.
            let _ = actor_ref.send_message(ToGossipManager::Unsubscribe(self.topic));
        }
    }
}
