// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::{HashMap, HashSet};

use anyhow::{Context, Result};
use futures_lite::StreamExt;
use futures_util::FutureExt;
use iroh_gossip::net::{Event, Gossip, GossipEvent, GossipReceiver, GossipSender, GossipTopic};
use iroh_net::key::PublicKey;
use p2panda_sync::TopicQuery;
use tokio::sync::mpsc;
use tokio::task::JoinSet;
use tokio_stream::StreamMap;
use tracing::{error, warn};

use crate::engine::ToEngineActor;

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
    Shutdown,
}

/// The `GossipActor` manages gossip topic membership (joining and leaving of topics) and
/// facilitates flows of messages into and out of individual gossip overlays.
pub struct GossipActor<T> {
    engine_actor_tx: mpsc::Sender<ToEngineActor<T>>,
    gossip: Gossip,
    gossip_events: StreamMap<[u8; 32], GossipReceiver>,
    gossip_senders: HashMap<[u8; 32], GossipSender>,
    inbox: mpsc::Receiver<ToGossipActor>,
    joined: HashSet<[u8; 32]>,
    pending_joins: JoinSet<([u8; 32], Result<GossipTopic>)>,
    want_join: HashSet<[u8; 32]>,
}

impl<T> GossipActor<T>
where
    T: TopicQuery + 'static,
{
    pub fn new(
        inbox: mpsc::Receiver<ToGossipActor>,
        gossip: Gossip,
        engine_actor_tx: mpsc::Sender<ToEngineActor<T>>,
    ) -> Self {
        Self {
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
                let gossip = self.gossip.clone();
                let fut = async move {
                    let stream = gossip.join(topic_id.into(), peers).await?;
                    Ok(stream)
                }
                .map(move |stream| (topic_id, stream));
                self.want_join.insert(topic_id);
                self.pending_joins.spawn(fut);
            }
            ToGossipActor::Leave { topic_id } => {
                // Quit the topic by dropping all handles to `GossipTopic` for the given topic id.
                let _handle = self.gossip_events.remove(&topic_id);
                self.joined.remove(&topic_id);
                self.want_join.remove(&topic_id);
            }
            ToGossipActor::Shutdown => {
                for topic_id in self.joined.iter() {
                    let _handle = self.gossip_events.remove(topic_id);
                }
                return Ok(false);
            }
        }

        Ok(true)
    }

    async fn on_gossip_event(&mut self, event: Option<([u8; 32], Result<Event>)>) -> Result<()> {
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
                        delivered_from: msg.delivered_from,
                        topic_id,
                    })
                    .await?;
            }
            GossipEvent::NeighborUp(peer) => {
                self.engine_actor_tx
                    .send(ToEngineActor::GossipNeighborUp { topic_id, peer })
                    .await?;
            }
            GossipEvent::Joined(_) => {
                // Not used currently
            }
            GossipEvent::NeighborDown(_) => {
                // Not used currently
            }
        }
        Ok(())
    }

    async fn on_joined(&mut self, topic_id: [u8; 32], stream: GossipTopic) -> Result<()> {
        self.joined.insert(topic_id);

        // Split the gossip stream and insert handles to the receiver and sender
        let (stream_tx, stream_rx) = stream.split();
        self.gossip_events.insert(topic_id, stream_rx);
        self.gossip_senders.insert(topic_id, stream_tx);

        self.engine_actor_tx
            .send(ToEngineActor::GossipJoined { topic_id })
            .await?;

        Ok(())
    }
}
