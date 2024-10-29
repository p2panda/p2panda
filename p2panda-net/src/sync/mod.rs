// SPDX-License-Identifier: AGPL-3.0-or-later

mod handler;
pub(crate) mod manager;

pub use handler::{SyncConnection, SYNC_CONNECTION_ALPN};

use std::sync::Arc;

use anyhow::Result;
use futures_util::{AsyncRead, AsyncWrite, SinkExt};
use iroh_net::key::PublicKey;
use p2panda_sync::{FromSync, SyncError, SyncProtocol, Topic};
use tokio::sync::mpsc;
use tokio_util::sync::PollSender;
use tracing::{debug, error};

use crate::engine::ToEngineActor;
use crate::TopicId;

/// Initiate a sync protocol session over the provided bi-directional stream for the given peer and
/// topic.
pub async fn initiate_sync<T, S, R>(
    mut send: &mut S,
    mut recv: &mut R,
    peer: PublicKey,
    topic: T,
    sync_protocol: Arc<dyn for<'a> SyncProtocol<'a, T> + 'static>,
    engine_actor_tx: mpsc::Sender<ToEngineActor<T>>,
) -> Result<()>
where
    T: Topic + TopicId + 'static,
    S: AsyncWrite + Send + Unpin,
    R: AsyncRead + Send + Unpin,
{
    debug!(
        "initiate sync session with peer {} over topic {:?}",
        peer, topic
    );

    // Set up a channel for receiving new application messages.
    let (tx, mut rx) = mpsc::channel::<FromSync<T>>(128);
    let mut sink = PollSender::new(tx).sink_map_err(|e| SyncError::Critical(e.to_string()));

    // Spawn a task which picks up any new application messages and sends them on to the engine
    // for handling.
    let mut sync_handshake_success = false;
    let topic_id = topic.id();
    tokio::spawn(async move {
        while let Some(message) = rx.recv().await {
            // We expect the first message to be a topic id
            if let FromSync::HandshakeSuccess(_) = &message {
                if sync_handshake_success {
                    error!("received handshake success message twice");
                    break;
                }
                sync_handshake_success = true;

                // Inform the engine that we are expecting sync messages from the peer on this topic
                engine_actor_tx
                    .send(ToEngineActor::SyncHandshakeSuccess { peer, topic_id })
                    .await
                    .expect("engine channel closed");

                continue;
            }

            let FromSync::Data(header, payload) = message else {
                error!("expected bytes from app message channel");
                return;
            };

            if let Err(err) = engine_actor_tx
                .send(ToEngineActor::SyncMessage {
                    header,
                    payload,
                    delivered_from: peer,
                    topic_id,
                })
                .await
            {
                error!("error in sync actor: {}", err)
            };
        }
        engine_actor_tx
            .send(ToEngineActor::SyncDone { peer, topic_id })
            .await
            .expect("engine channel closed");
    });

    // Run the sync protocol.
    let result = sync_protocol
        .initiate(
            topic,
            Box::new(&mut send),
            Box::new(&mut recv),
            Box::new(&mut sink),
        )
        .await;

    if let Err(err) = result {
        error!("sync protocol initiation failed: {err}");
    }

    Ok(())
}

/// Accept a sync protocol session over the provided bi-directional stream for the given peer and
/// topic.
pub async fn accept_sync<T, S, R>(
    mut send: &mut S,
    mut recv: &mut R,
    peer: PublicKey,
    sync_protocol: Arc<dyn for<'a> SyncProtocol<'a, T> + 'static>,
    engine_actor_tx: mpsc::Sender<ToEngineActor<T>>,
) -> Result<()>
where
    T: Topic + TopicId + 'static,
    S: AsyncWrite + Send + Unpin,
    R: AsyncRead + Send + Unpin,
{
    debug!("accept sync session with peer {}", peer);

    // Set up a channel for receiving new application messages.
    let (tx, mut rx) = mpsc::channel::<FromSync<T>>(128);
    let mut sink = PollSender::new(tx).sink_map_err(|e| SyncError::Critical(e.to_string()));

    // Spawn a task which picks up any new application messages and sends them on to the engine
    // for handling.
    tokio::spawn(async move {
        let mut topic_id = None;
        while let Some(message) = rx.recv().await {
            // We expect the first message to be a topic id
            if let FromSync::HandshakeSuccess(topic) = &message {
                // It should only be sent once so topic should be None now
                if topic_id.is_some() {
                    error!("topic message already received");
                    break;
                }

                // Set the topic id
                topic_id = Some(topic.id().to_owned());

                // Inform the engine that we are expecting sync messages from the peer on this topic
                engine_actor_tx
                    .send(ToEngineActor::SyncHandshakeSuccess {
                        peer,
                        topic_id: topic.id(),
                    })
                    .await
                    .expect("engine channel closed");

                continue;
            }

            // If topic id wasn't set yet error here as it must be known to process further messages
            let Some(topic_id) = topic_id else {
                error!("topic id not received");
                return;
            };

            let FromSync::Data(header, payload) = message else {
                error!("expected message bytes");
                return;
            };

            if let Err(err) = engine_actor_tx
                .send(ToEngineActor::SyncMessage {
                    header,
                    payload,
                    delivered_from: peer,
                    topic_id,
                })
                .await
            {
                error!("error in sync actor: {}", err)
            };
        }

        // If topic was never set we didn't receive any messages and so the engine was not
        // informed it should buffer messages and we can return here.
        let Some(topic_id) = topic_id else {
            return;
        };

        engine_actor_tx
            .send(ToEngineActor::SyncDone { peer, topic_id })
            .await
            .expect("engine channel closed");
    });

    // Run the sync protocol.
    let result = sync_protocol
        .accept(
            Box::new(&mut send),
            Box::new(&mut recv),
            Box::new(&mut sink),
        )
        .await;

    if let Err(err) = result {
        error!("sync protocol accept failed: {err}");
    }

    Ok(())
}
