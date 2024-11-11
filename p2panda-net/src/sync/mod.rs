// SPDX-License-Identifier: AGPL-3.0-or-later

mod handler;
pub(crate) mod manager;

pub use handler::{SyncConnection, SYNC_CONNECTION_ALPN};

use std::sync::Arc;

use anyhow::Result;
use futures_util::{AsyncRead, AsyncWrite, SinkExt};
use iroh_net::key::PublicKey;
use p2panda_sync::{FromSync, SyncError, SyncProtocol, Topic};
use tokio::sync::{mpsc, oneshot};
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
) -> Result<(), SyncError>
where
    T: Topic + TopicId + 'static,
    S: AsyncWrite + Send + Unpin,
    R: AsyncRead + Send + Unpin,
{
    debug!(
        "initiate sync session with peer {} over topic {:?}",
        peer, topic
    );

    engine_actor_tx
        .send(ToEngineActor::SyncStart {
            topic: topic.clone(),
            peer,
        })
        .await
        .expect("engine channel closed");

    // Set up a channel for receiving new application messages.
    let (tx, mut rx) = mpsc::channel::<FromSync<T>>(128);
    let mut sink = PollSender::new(tx).sink_map_err(|e| SyncError::Critical(e.to_string()));

    // Spawn a task which picks up any new application messages and sends them on to the engine
    // for handling.
    {
        let engine_actor_tx = engine_actor_tx.clone();
        let mut sync_handshake_success = false;
        let topic = topic.clone();

        tokio::spawn(async move {
            while let Some(message) = rx.recv().await {
                // 1. Handshake Phase.
                // ~~~~~~~~~~~~~~~~~~~
                //
                // At the beginning of every sync session the "initiating" peer needs to send over
                // the topic to the "accepting" peer during the handshake phase. This is the first
                // message we're expecting:
                if let FromSync::HandshakeSuccess(_) = &message {
                    if sync_handshake_success {
                        error!("received handshake success message twice");
                        break;
                    }
                    sync_handshake_success = true;

                    // Inform the engine that we are expecting sync messages from the peer on this
                    // topic.
                    engine_actor_tx
                        .send(ToEngineActor::SyncHandshakeSuccess {
                            peer,
                            topic: topic.clone(),
                        })
                        .await
                        .expect("engine channel closed");

                    continue;
                }

                // 2. Data Sync Phase.
                // ~~~~~~~~~~~~~~~~~~~
                let FromSync::Data(header, payload) = message else {
                    error!("expected bytes from app message channel");
                    return;
                };

                if let Err(err) = engine_actor_tx
                    .send(ToEngineActor::SyncMessage {
                        header,
                        payload,
                        delivered_from: peer,
                        topic: topic.clone(),
                    })
                    .await
                {
                    error!("error in sync actor: {}", err)
                };
            }

            engine_actor_tx
                .send(ToEngineActor::SyncDone { peer, topic })
                .await
                .expect("engine channel closed");
        });
    }

    // Run the sync protocol.
    sync_protocol
        .initiate(
            topic.clone(),
            Box::new(&mut send),
            Box::new(&mut recv),
            Box::new(&mut sink),
        )
        .await?;

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
) -> Result<(), SyncError>
where
    T: Topic + TopicId + 'static,
    S: AsyncWrite + Send + Unpin,
    R: AsyncRead + Send + Unpin,
{
    debug!("accept sync session with peer {}", peer);

    // Set up a channel for receiving new application messages.
    let (tx, mut rx) = mpsc::channel::<FromSync<T>>(128);
    let mut sink = PollSender::new(tx).sink_map_err(|e| SyncError::Critical(e.to_string()));

    // Set up a channel for sending over errors to the task which happened during sync.
    let (sync_error_tx, mut sync_error_rx) = oneshot::channel::<SyncError>();

    // Spawn a task which picks up any new application messages and sends them on to the engine
    // for handling.
    tokio::spawn(async move {
        let mut topic = None;

        loop {
            tokio::select! {
                biased;
                Ok(_) = &mut sync_error_rx => {
                    engine_actor_tx
                        .send(ToEngineActor::SyncFailed {
                            peer,
                            topic: topic.clone(),
                        })
                        .await
                        .expect("engine channel closed");
                },
                Some(message) = rx.recv() => {
                    // 1. Handshake Phase.
                    // ~~~~~~~~~~~~~~~~~~~
                    //
                    // At the beginning of every sync session the "accepting" peer needs to learn the
                    // topic of the "initiating" peer during the handshake phase. This is the first
                    // message we're expecting:
                    if let FromSync::HandshakeSuccess(handshake_topic) = &message {
                        // It should only be sent once so topic should be `None` now.
                        if topic.is_some() {
                            error!("topic message already received");
                            break;
                        }

                        topic = Some(handshake_topic.clone());

                        // Inform the engine that we are expecting sync messages from the peer on
                        // this topic.
                        engine_actor_tx
                            .send(ToEngineActor::SyncHandshakeSuccess {
                                peer,
                                topic: handshake_topic.clone(),
                            })
                            .await
                            .expect("engine channel closed");

                        continue;
                    }

                    // 2. Data Sync Phase.
                    // ~~~~~~~~~~~~~~~~~~~
                    //
                    // The topic must be known at this point in order to process further messages.
                    let Some(topic) = &topic else {
                        error!("topic not received");
                        return;
                    };

                    let FromSync::Data(header, payload) = message else {
                        error!("expected message bytes");
                        return;
                    };

                    engine_actor_tx
                        .send(ToEngineActor::SyncMessage {
                            header,
                            payload,
                            delivered_from: peer,
                            topic: topic.clone(),
                        })
                        .await
                        .expect("engine channel closed");
                },
                else => {
                    break;
                }
            }
        }

        // If topic was never set then we didn't receive any messages. In that case, the engine
        // wasn't informed it should buffer messages and so we can return here.
        let Some(topic) = topic else {
            return;
        };

        engine_actor_tx
            .send(ToEngineActor::SyncDone { peer, topic })
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

    if let Err(sync_err) = result {
        sync_error_tx
            .send(sync_err)
            .expect("error oneshot sender failed");
    }

    Ok(())
}
