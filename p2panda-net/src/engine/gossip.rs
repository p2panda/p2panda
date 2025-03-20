// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::{HashMap, HashSet};

use anyhow::{Context, Result};
use futures_lite::StreamExt;
use iroh_gossip::net::{
    Error as GossipError, Event, Gossip, GossipEvent, GossipReceiver, GossipSender, GossipTopic,
};
use p2panda_core::PublicKey;
use p2panda_sync::TopicQuery;
use tokio::sync::mpsc;
use tokio::task::JoinSet;
use tokio_stream::StreamMap;
use tracing::{error, warn};

use crate::engine::ToEngineActor;
use crate::{from_public_key, to_public_key};

#[derive(Debug)]
pub enum ToGossipActor {
    Broadcast {
        topic_id: [u8; 32],
        bytes: Vec<u8>,
    },
    Join {
        topic_id: [u8; 32],
        peers: Vec<PublicKey>,
    },
    #[allow(dead_code)]
    Leave {
        topic_id: [u8; 32],
    },
    Reset,
    Shutdown,
}

/// The `GossipActor` manages gossip topic membership (joining and leaving of topics) and
/// facilitates flows of messages into and out of individual gossip overlays.
pub struct GossipActor<T> {
    bootstrap: bool,
    engine_actor_tx: mpsc::Sender<ToEngineActor<T>>,
    gossip: Gossip,
    gossip_events: StreamMap<[u8; 32], GossipReceiver>,
    gossip_senders: HashMap<[u8; 32], GossipSender>,
    inbox: mpsc::Receiver<ToGossipActor>,
    joined: HashSet<[u8; 32]>,
    pending_joins: JoinSet<([u8; 32], Result<GossipTopic, GossipError>)>,
    want_join: HashSet<[u8; 32]>,
}

impl<T> GossipActor<T>
where
    T: TopicQuery + 'static,
{
    pub fn new(
        bootstrap: bool,
        inbox: mpsc::Receiver<ToGossipActor>,
        gossip: Gossip,
        engine_actor_tx: mpsc::Sender<ToEngineActor<T>>,
    ) -> Self {
        Self {
            bootstrap,
            engine_actor_tx,
            gossip,
            gossip_events: Default::default(),
            gossip_senders: Default::default(),
            inbox,
            joined: Default::default(),
            pending_joins: Default::default(),
            want_join: Default::default(),
        }
    }

    pub async fn run(&mut self) -> Result<()> {
        loop {
            tokio::select! {
                next = self.gossip_events.next(), if !self.gossip_events.is_empty() => {
                    if let Err(err) = self.on_gossip_event(next).await {
                        error!("gossip actor died: {err:?}");
                        return Err(err);
                    }
                },
                msg = self.inbox.recv() => {
                    let msg = msg.context("inbox closed")?;
                    if !self.on_actor_message(msg).await.context("on_actor_message")? {
                        break;
                    }
                },
                Some(res) = self.pending_joins.join_next(), if !self.pending_joins.is_empty() => {
                    let (topic, res) = res.context("pending_joins closed")?;
                    match res {
                        Ok(stream) => {
                            self.on_joined(topic, stream).await?;
                        },
                        Err(err) => {
                            if self.want_join.contains(&topic) {
                                error!(?topic, ?err, "failed to join gossip");
                            }
                        }
                    }
                },
            }
        }

        Ok(())
    }

    async fn on_actor_message(&mut self, msg: ToGossipActor) -> Result<bool> {
        match msg {
            ToGossipActor::Broadcast { topic_id, bytes } => {
                if let Some(gossip_tx) = self.gossip_senders.get(&topic_id) {
                    if let Err(err) = gossip_tx.broadcast(bytes.into()).await {
                        error!(
                            topic_id = "{topic_id:?}",
                            "failed to broadcast gossip msg: {}", err
                        )
                    }
                }
            }
            ToGossipActor::Join { topic_id, peers } => {
                // Only prevent this join attempt if our node is not acting as a bootstrap node
                // and a subsequent join attempt has already been made.
                if !self.bootstrap && self.want_join.contains(&topic_id) {
                    return Ok(true);
                }

                let gossip = self.gossip.clone();
                let peers = peers
                    .iter()
                    .map(|key: &p2panda_core::PublicKey| from_public_key(*key))
                    .collect();
                let fut = async move {
                    let stream = gossip.subscribe_and_join(topic_id.into(), peers).await;
                    (topic_id, stream)
                };

                self.want_join.insert(topic_id);
                self.pending_joins.spawn(fut);
            }
            ToGossipActor::Leave { topic_id } => {
                // Quit the topic by dropping all handles to `GossipTopic` for the given topic id.
                let _handle = self.gossip_events.remove(&topic_id);
                self.joined.remove(&topic_id);
                self.want_join.remove(&topic_id);
            }
            ToGossipActor::Reset => self.want_join.clear(),
            ToGossipActor::Shutdown => {
                for topic_id in self.joined.iter() {
                    let _handle = self.gossip_events.remove(topic_id);
                }
                return Ok(false);
            }
        }

        Ok(true)
    }

    async fn on_gossip_event(
        &mut self,
        event: Option<([u8; 32], Result<Event, GossipError>)>,
    ) -> Result<()> {
        let (topic_id, event) = event.context("gossip event channel closed")?;
        let event = match event {
            Ok(Event::Gossip(event)) => event,
            Ok(Event::Lagged) => {
                warn!("missed gossip messages - dropping gossip event");
                return Ok(());
            }
            Err(err) => {
                error!(topic_id = "{topic_id:?}", "gossip receiver error: {}", err);
                return Ok(());
            }
        };

        if !self.joined.contains(&topic_id) && !self.want_join.contains(&topic_id) {
            error!(
                topic_id = "{topic_id:?}",
                "received gossip event for unknown topic"
            );
            return Ok(());
        }

        if let Err(err) = self.on_gossip_event_inner(topic_id, event).await {
            error!(
                topic_id = "{topic_id:?}",
                ?err,
                "failed to process gossip event"
            );
        }

        Ok(())
    }

    async fn on_gossip_event_inner(
        &mut self,
        topic_id: [u8; 32],
        event: GossipEvent,
    ) -> Result<()> {
        match event {
            GossipEvent::Received(msg) => {
                self.engine_actor_tx
                    .send(ToEngineActor::GossipMessage {
                        bytes: msg.content.into(),
                        delivered_from: to_public_key(msg.delivered_from),
                        topic_id,
                    })
                    .await?;
            }
            GossipEvent::NeighborUp(peer) => {
                self.engine_actor_tx
                    .send(ToEngineActor::GossipNeighborUp {
                        topic_id,
                        peer: to_public_key(peer),
                    })
                    .await?;
            }
            GossipEvent::Joined(_peers) => {
                // We send this event to the engine actor in `on_joined()`.
            }
            GossipEvent::NeighborDown(peer) => {
                self.engine_actor_tx
                    .send(ToEngineActor::GossipNeighborDown {
                        topic_id,
                        peer: to_public_key(peer),
                    })
                    .await?;
            }
        }
        Ok(())
    }

    async fn on_joined(&mut self, topic_id: [u8; 32], stream: GossipTopic) -> Result<()> {
        self.joined.insert(topic_id);

        // Split the gossip stream and insert handles to the receiver and sender.
        let (stream_tx, stream_rx) = stream.split();

        // Collect all our current direct neighbors for this gossip topic.
        let peers: Vec<PublicKey> = stream_rx.neighbors().map(to_public_key).collect();

        self.gossip_events.insert(topic_id, stream_rx);
        self.gossip_senders.insert(topic_id, stream_tx);

        self.engine_actor_tx
            .send(ToEngineActor::GossipJoined { topic_id, peers })
            .await?;

        Ok(())
    }
}
