// SPDX-License-Identifier: AGPL-3.0-or-later

use std::sync::Arc;

use anyhow::Result;
use futures_util::{AsyncRead, AsyncWrite, SinkExt};
use iroh_net::key::PublicKey;
use p2panda_sync::{FromSync, SyncError, SyncProtocol, Topic};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_util::sync::PollSender;
use tracing::{debug, error, warn};

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
        .map_err(|err| {
            SyncError::Critical(format!("engine_actor_tx failed sending sync start: {err}"))
        })?;

    // Set up a channel for receiving messages from the sync session.
    let (tx, mut rx) = mpsc::channel::<FromSync<T>>(128);
    let mut sink = PollSender::new(tx).sink_map_err(|e| SyncError::Critical(e.to_string()));

    // Spawn a "glue" task which represents the layer between the sync session and the engine.
    //
    // It picks up any messages from the sync session makes sure that the "Two-Phase Sync Flow" is
    // followed (I. "Handshake" Phase & II. "Data Sync" Phase) and the engine accordingly informed
    // about it.
    //
    // If the task detects any invalid behaviour of the sync flow, it fails critically, indicating
    // that the sync protocol implementation does not behave correctly and is not compatible with
    // the engine.
    //
    // Additionally, the task forwards any synced application data straight to the engine.
    let glue_task_handle: JoinHandle<Result<(), SyncError>> = {
        let mut sync_handshake_success = false;
        let topic = topic.clone();

        tokio::spawn(async move {
            while let Some(message) = rx.recv().await {
                // I. Handshake Phase.
                //
                // At the beginning of every sync session the "initiating" peer needs to send over
                // the topic to the "accepting" peer during the handshake phase. This is the first
                // message we're expecting:
                if let FromSync::HandshakeSuccess(_) = message {
                    // Receiving the handshake message twice is a protocol violation.
                    if sync_handshake_success {
                        return Err(SyncError::Critical(
                            "received handshake message twice from sync session in handshake phase"
                                .into(),
                        ));
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
                        .map_err(|err| {
                            SyncError::Critical(format!(
                                "engine_actor_tx failed sending sync handshake success: {err}"
                            ))
                        })?;

                    continue;
                }

                // 2. Data Sync Phase.
                // ~~~~~~~~~~~~~~~~~~~
                let FromSync::Data(header, payload) = message else {
                    return Err(SyncError::Critical("expected to receive only data messages from sync session in data sync phase".into()));
                };

                engine_actor_tx
                    .send(ToEngineActor::SyncMessage {
                        header,
                        payload,
                        delivered_from: peer,
                        topic: topic.clone(),
                    })
                    .await
                    .map_err(|err| {
                        SyncError::Critical(format!(
                            "engine_actor_tx failed sending sync message: {err}"
                        ))
                    })?;
            }

            engine_actor_tx
                .send(ToEngineActor::SyncDone { peer, topic })
                .await
                .map_err(|err| {
                    SyncError::Critical(format!("engine_actor_tx failed sending sync done: {err}"))
                })?;

            Ok(())
        })
    };

    // Run the "initiating peer" side of the sync protocol.
    let result = sync_protocol
        .initiate(
            topic,
            Box::new(&mut send),
            Box::new(&mut recv),
            Box::new(&mut sink),
        )
        .await;

    // Drop the tx, so the rx in the glue task receives the closing event.
    drop(sink);

    // We're expecting the task to exit with a result soon, we're awaiting it here ..
    let glue_task_result = glue_task_handle
        .await
        .map_err(|err| SyncError::Critical(format!("glue task handle failed: {err}")))?;

    // .. to inform some brave developer who will read the error logs ..
    if let Err(SyncError::Critical(err)) = &glue_task_result {
        error!("critical error in sync protocol: {err}");
    }

    // .. and forward it further.
    glue_task_result?;

    // The same we're doing with errors coming from the sync protocol implementation itself.
    if let Err(err) = result {
        match &err {
            SyncError::Critical(err) => {
                error!("critical error in sync protocol: {err}");
            }
            _ => {
                warn!("error in sync protocol: {err}");
            }
        }
        return Err(err);
    }

    Ok(())
}
