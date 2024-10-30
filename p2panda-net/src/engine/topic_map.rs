// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use anyhow::{Context, Result};
use iroh_net::key::PublicKey;
use p2panda_sync::Topic;
use tokio::sync::{broadcast, oneshot, RwLock};
use tracing::warn;

use crate::network::FromNetwork;
use crate::{to_public_key, TopicId};

/// The topic associated with a particular subscription along with it's broadcast channel and
/// oneshot ready channel.
type TopicMeta<T> = (
    T,
    broadcast::Sender<FromNetwork>,
    Option<oneshot::Sender<()>>,
);

#[derive(Clone, Debug)]
pub struct TopicMap<T> {
    inner: Arc<RwLock<TopicMapInner<T>>>,
}

#[derive(Debug)]
struct TopicMapInner<T> {
    earmarked: HashMap<[u8; 32], TopicMeta<T>>,
    pending_joins: HashSet<[u8; 32]>,
    joined: HashSet<[u8; 32]>,
}

impl<T> TopicMap<T>
where
    T: Topic + TopicId,
{
    /// Generate an empty topic map.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(TopicMapInner {
                earmarked: HashMap::new(),
                pending_joins: HashSet::new(),
                joined: HashSet::new(),
            })),
        }
    }

    pub async fn get(&self, topic_id: &[u8; 32]) -> Option<T> {
        let inner = self.inner.read().await;
        inner
            .earmarked
            .get(topic_id)
            .map(|(topic, _, _)| topic.clone())
    }

    /// Mark a topic of interest to our node.
    pub async fn earmark(
        &mut self,
        topic: T,
        from_network_tx: broadcast::Sender<FromNetwork>,
        gossip_ready_tx: oneshot::Sender<()>,
    ) {
        let mut inner = self.inner.write().await;
        inner.earmarked.insert(
            topic.id(),
            (topic.clone(), from_network_tx, Some(gossip_ready_tx)),
        );
        inner.pending_joins.insert(topic.id());
    }

    /// Remove a topic of interest to our node.
    pub async fn remove_earmark(&mut self, topic_id: &[u8; 32]) {
        let mut inner = self.inner.write().await;
        inner.earmarked.remove(topic_id);
        inner.pending_joins.remove(topic_id);
    }

    /// Return a list of topics of interest to our node.
    pub async fn earmarked(&self) -> Vec<[u8; 32]> {
        let inner = self.inner.read().await;
        inner.earmarked.keys().cloned().collect()
    }

    /// Mark that we've successfully joined a gossip overlay for this topic.
    pub async fn set_joined(&mut self, topic_id: [u8; 32]) -> Result<()> {
        let mut inner = self.inner.write().await;
        if inner.pending_joins.remove(&topic_id) {
            inner.joined.insert(topic_id);

            // Inform local topic subscribers that the gossip overlay has been joined and is ready
            // for messages.
            if let Some((_, _, gossip_ready_tx)) = inner.earmarked.get_mut(&topic_id) {
                // We need the `Sender` to be owned so we take it and replace with `None`.
                if let Some(oneshot_tx) = gossip_ready_tx.take() {
                    if oneshot_tx.send(()).is_err() {
                        warn!("gossip topic oneshot ready receiver dropped")
                    }
                }
            }
        }

        Ok(())
    }

    /// Return true if we've successfully joined a gossip overlay for this topic.
    pub async fn has_successfully_joined(&self, topic_id: &[u8; 32]) -> bool {
        let inner = self.inner.read().await;
        inner.joined.contains(topic_id)
    }

    /// Return true if there's either a pending or successfully joined gossip overlay for this
    /// topic.
    pub async fn has_joined(&self, topic_id: &[u8; 32]) -> bool {
        let inner = self.inner.read().await;
        inner.joined.contains(topic_id) || inner.pending_joins.contains(topic_id)
    }

    /// Handle incoming messages from gossip.
    ///
    /// This method forwards messages to the subscribers for the given topic.
    pub async fn on_gossip_message(
        &self,
        topic_id: [u8; 32],
        bytes: Vec<u8>,
        delivered_from: PublicKey,
    ) -> Result<()> {
        let inner = self.inner.read().await;
        let (_, from_network_tx, _gossip_ready_tx) = inner
            .earmarked
            .get(&topic_id)
            .context("on_gossip_message")?;
        from_network_tx.send(FromNetwork::GossipMessage {
            bytes,
            delivered_from: to_public_key(delivered_from),
        })?;
        Ok(())
    }

    /// Handle incoming messages from sync.
    ///
    /// This method forwards messages to the subscribers for the given topic.
    pub async fn on_sync_message(
        &self,
        topic_id: [u8; 32],
        header: Vec<u8>,
        payload: Option<Vec<u8>>,
        delivered_from: PublicKey,
    ) -> Result<()> {
        let inner = self.inner.read().await;
        let (_, from_network_tx, _) = inner.earmarked.get(&topic_id).context("on_sync_message")?;
        from_network_tx.send(FromNetwork::SyncMessage {
            header,
            payload,
            delivered_from: to_public_key(delivered_from),
        })?;
        Ok(())
    }
}
