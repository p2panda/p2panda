// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::{HashMap, HashSet};

use anyhow::{Context, Result};
use futures_lite::StreamExt;
use futures_util::FutureExt;
use iroh_gossip::net::{Event, Gossip, GossipEvent, GossipReceiver, GossipSender, GossipTopic};
use iroh_gossip::proto::TopicId;
use iroh_net::key::PublicKey;
use tokio::sync::mpsc;
use tokio::task::JoinSet;
use tokio_stream::StreamMap;
use tracing::{error, warn};

use crate::engine::ToEngineActor;
use crate::Topic;

#[derive(Debug)]
pub enum ToGossipActor {
    Broadcast {
        topic: TopicId,
        bytes: Vec<u8>,
    },
    Join {
        topic: TopicId,
        peers: Vec<PublicKey>,
    },
    Leave {
        topic: TopicId,
    },
    Shutdown,
}

pub struct GossipActor<T> {
    engine_actor_tx: mpsc::Sender<ToEngineActor<T>>,
    gossip: Gossip,
    gossip_events: StreamMap<TopicId, GossipReceiver>,
    gossip_senders: HashMap<TopicId, GossipSender>,
    inbox: mpsc::Receiver<ToGossipActor>,
    joined: HashSet<TopicId>,
    pending_joins: JoinSet<(TopicId, Result<GossipTopic>)>,
    want_join: HashSet<TopicId>,
}

impl<T> GossipActor<T>
where
    T: Topic + 'static,
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
            ToGossipActor::Broadcast { topic, bytes } => {
                if let Some(gossip_tx) = self.gossip_senders.get(&topic) {
                    if let Err(err) = gossip_tx.broadcast(bytes.into()).await {
                        error!(topic = %topic, "failed to broadcast gossip msg: {}", err)
                    }
                }
            }
            ToGossipActor::Join { topic, peers } => {
                let gossip = self.gossip.clone();
                let fut = async move {
                    let stream = gossip.join(topic, peers).await?;

                    Ok(stream)
                }
                .map(move |stream| (topic, stream));
                self.want_join.insert(topic);
                self.pending_joins.spawn(fut);
            }
            ToGossipActor::Leave { topic } => {
                // Quit the topic by dropping all handles to `GossipTopic` for the given topic
                let _handle = self.gossip_events.remove(&topic);
                self.joined.remove(&topic);
                self.want_join.remove(&topic);
            }
            ToGossipActor::Shutdown => {
                for topic in self.joined.iter() {
                    let _handle = self.gossip_events.remove(topic);
                }
                return Ok(false);
            }
        }

        Ok(true)
    }

    async fn on_gossip_event(&mut self, event: Option<(TopicId, Result<Event>)>) -> Result<()> {
        let (topic, event) = event.context("gossip event channel closed")?;
        let event = match event {
            Ok(Event::Gossip(event)) => event,
            Ok(Event::Lagged) => {
                warn!("missed gossip messages - dropping gossip event");
                return Ok(());
            }
            Err(err) => {
                error!(topic = %topic, "gossip receiver error: {}", err);
                return Ok(());
            }
        };

        if !self.joined.contains(&topic) && !self.want_join.contains(&topic) {
            error!(topic = %topic, "received gossip event for unknown topic");
            return Ok(());
        }

        if let Err(err) = self.on_gossip_event_inner(topic, event).await {
            error!(topic = %topic, ?err, "failed to process gossip event");
        }

        Ok(())
    }

    async fn on_gossip_event_inner(&mut self, topic: TopicId, event: GossipEvent) -> Result<()> {
        match event {
            GossipEvent::Received(msg) => {
                self.engine_actor_tx
                    .send(ToEngineActor::Received {
                        bytes: msg.content.into(),
                        delivered_from: msg.delivered_from,
                        topic,
                    })
                    .await?;
            }
            GossipEvent::NeighborUp(peer) => {
                self.engine_actor_tx
                    .send(ToEngineActor::NeighborUp { topic, peer })
                    .await?;
            }
            // @TODO: Unmatched variants are `Joined(Vec<NodeId>)` and `Received(Message)`
            _ => (),
        }
        Ok(())
    }

    async fn on_joined(&mut self, topic: TopicId, stream: GossipTopic) -> Result<()> {
        self.joined.insert(topic);

        // Split the gossip stream and insert handles to the receiver and sender
        let (stream_tx, stream_rx) = stream.split();
        self.gossip_events.insert(topic, stream_rx);
        self.gossip_senders.insert(topic, stream_tx);

        self.engine_actor_tx
            .send(ToEngineActor::TopicJoined { topic })
            .await?;

        Ok(())
    }
}
