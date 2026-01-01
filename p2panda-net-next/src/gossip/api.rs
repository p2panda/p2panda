// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashMap;
use std::sync::Arc;

use p2panda_discovery::address_book::NodeInfo as _;
use ractor::{ActorRef, call};
use thiserror::Error;
use tokio::sync::{RwLock, broadcast, mpsc};

use crate::address_book::{AddressBook, AddressBookError};
use crate::gossip::actors::ToGossipManager;
use crate::gossip::builder::Builder;
use crate::gossip::events::GossipEvent;
use crate::iroh_endpoint::Endpoint;
use crate::{NodeId, TopicId};

/// Mapping of topic to the associated sender channels for getting messages into and out of the
/// gossip overlay.
type GossipSenders = HashMap<TopicId, (mpsc::Sender<Vec<u8>>, broadcast::Sender<Vec<u8>>)>;

#[derive(Clone)]
pub struct Gossip {
    my_node_id: NodeId,
    address_book: AddressBook,
    inner: Arc<RwLock<Inner>>,
}

struct Inner {
    senders: GossipSenders,
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
            inner: Arc::new(RwLock::new(Inner {
                senders: HashMap::new(),
                actor_ref,
            })),
        }
    }

    pub fn builder(address_book: AddressBook, endpoint: Endpoint) -> Builder {
        Builder::new(address_book, endpoint)
    }

    pub async fn stream(&self, topic: TopicId) -> Result<EphemeralStream, GossipError> {
        if let Some((to_gossip_tx, from_gossip_tx)) = self.inner.read().await.senders.get(&topic) {
            Ok(EphemeralStream::new(
                topic,
                to_gossip_tx.clone(),
                from_gossip_tx.clone(),
            ))
        } else {
            let mut inner = self.inner.write().await;

            let node_ids = {
                let node_infos = self
                    .address_book
                    .node_infos_by_ephemeral_messaging_topics([topic])
                    .await?;
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
                call!(inner.actor_ref, ToGossipManager::Subscribe, topic, node_ids)?;

            // Store the gossip senders.
            //
            // `from_gossip_tx` is used to create a broadcast receiver when the user calls
            // `subscribe()` on `EphemeralStream`.
            inner
                .senders
                .insert(topic, (to_gossip_tx.clone(), from_gossip_tx.clone()));

            self.address_book
                .set_ephemeral_messaging_topics(self.my_node_id, inner.senders.keys().cloned())
                .await?;

            Ok(EphemeralStream::new(topic, to_gossip_tx, from_gossip_tx))
        }
    }

    /// Subscribe to system events.
    pub async fn events(&self) -> Result<broadcast::Receiver<GossipEvent>, GossipError> {
        let inner = self.inner.read().await;
        let result = call!(inner.actor_ref, ToGossipManager::Events)?;
        Ok(result)
    }
}

impl Drop for Inner {
    fn drop(&mut self) {
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
    ActorRpc(#[from] ractor::RactorErr<ToGossipManager>),

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
pub struct EphemeralStream {
    topic: TopicId,
    to_topic_tx: mpsc::Sender<Vec<u8>>,
    from_gossip_tx: broadcast::Sender<Vec<u8>>,
}

// TODO: Implement `Sink` for `EphemeralStream`.
impl EphemeralStream {
    pub(crate) fn new(
        topic: TopicId,
        to_topic_tx: mpsc::Sender<Vec<u8>>,
        from_gossip_tx: broadcast::Sender<Vec<u8>>,
    ) -> Self {
        Self {
            topic,
            to_topic_tx,
            from_gossip_tx,
        }
    }

    /// Publishes a message to the stream.
    pub async fn publish(&self, bytes: impl Into<Vec<u8>>) -> Result<(), EphemeralStreamError> {
        self.to_topic_tx
            .send(bytes.into())
            .await
            .map_err(|err| EphemeralStreamError::Publish(err.to_string()))?;
        Ok(())
    }

    /// Subscribes to the stream.
    ///
    /// The returned `EphemeralSubscription` provides a means of receiving messages from the
    /// stream.
    pub fn subscribe(&self) -> EphemeralSubscription {
        EphemeralSubscription::new(self.topic, self.from_gossip_tx.subscribe())
    }

    /// Returns the topic of the stream.
    pub fn topic(&self) -> TopicId {
        self.topic
    }
}

/// A handle to an ephemeral messaging stream subscription.
///
/// The stream can be used to receive messages from the stream.
pub struct EphemeralSubscription {
    topic: TopicId,
    from_topic_rx: broadcast::Receiver<Vec<u8>>,
}

// TODO: Implement `Stream` for `BroadcastReceiver`.
impl EphemeralSubscription {
    /// Returns a handle to an ephemeral messaging stream subscriber.
    pub(crate) fn new(topic: TopicId, from_topic_rx: broadcast::Receiver<Vec<u8>>) -> Self {
        Self {
            topic,
            from_topic_rx,
        }
    }

    /// Receives the next message from the stream.
    pub async fn recv(&mut self) -> Result<Vec<u8>, EphemeralStreamError> {
        self.from_topic_rx
            .recv()
            .await
            .map_err(EphemeralStreamError::Recv)
    }

    /// Attempts to return a pending value on this receiver without awaiting.
    pub fn try_recv(&mut self) -> Result<Vec<u8>, EphemeralStreamError> {
        self.from_topic_rx
            .try_recv()
            .map_err(EphemeralStreamError::TryRecv)
    }

    /// Returns the topic of the stream.
    pub fn topic(&self) -> TopicId {
        self.topic
    }
}

#[derive(Debug, Error)]
pub enum EphemeralStreamError {
    #[error(transparent)]
    Recv(#[from] broadcast::error::RecvError),

    #[error(transparent)]
    TryRecv(#[from] broadcast::error::TryRecvError),

    #[error("{0}")]
    Publish(String),
}
