// SPDX-License-Identifier: MIT OR Apache-2.0

use std::sync::Arc;

use anyhow::Result;
use futures_util::{AsyncRead, AsyncWrite, SinkExt};
use p2panda_core::PublicKey;
use p2panda_sync::{FromSync, SyncError, SyncProtocol, TopicQuery};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_util::sync::PollSender;
use tracing::{debug, error, warn};

use crate::engine::ToEngineActor;

/// Initiate a sync protocol session over the provided bi-directional stream for the given peer and
/// topic.
///
/// While this method "drives" the sync protocol implementation it also follows the "2-Phase
/// Protocol Flow" required for the engine to work efficiently. We're expecting the following
/// messages from this "initiator" flow:
///
/// 1. `SyncStart`: The sync session just began, we already know the TopicQuery since we're the
///    initiators.
/// 2. `SyncHandshakeSuccess`: We've successfully completed the I. "Handshake" phase, transmitting
///    the topic to the acceptor.
/// 3. `SyncMessage` (optional): The actual data we've exchanged with the other peer, this message
///    can occur never or multiple times, depending on how much data was sent.
/// 4. `SyncDone` We've successfully finished this session.
///
/// In case of a detected failure (either through an critical error on our end or an unexpected
/// behaviour from the remote peer), the initiator is _not_ sending a `SyncDone` message. A
/// `SyncFailed` message will be sent instead. This is handled in the sync actor.
///
/// Errors can be roughly categorized by:
///
/// 1. Critical system failures (bug in p2panda code or sync implementation, sync implementation
///    did not follow "2. Phase Flow" requirements, lack of system resources, etc.)
/// 2. Unexpected Behaviour (remote peer abruptly disconnected, error which got correctly handled
///    in sync implementation, etc.)
pub async fn initiate_sync<T, S, R>(
    mut send: &mut S,
    mut recv: &mut R,
    peer: PublicKey,
    topic: T,
    sync_protocol: Arc<dyn for<'a> SyncProtocol<'a, T> + 'static>,
    engine_actor_tx: mpsc::Sender<ToEngineActor<T>>,
) -> Result<(), SyncError>
where
    T: TopicQuery + 'static,
    S: AsyncWrite + Send + Unpin,
    R: AsyncRead + Send + Unpin,
{
    debug!(
        "initiate sync session with peer {} over topic {:?}",
        peer, topic
    );

    engine_actor_tx
        .send(ToEngineActor::SyncStart {
            topic: Some(topic.clone()),
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
    // It picks up any messages from the sync session, making sure that the "Two-Phase Sync Flow"
    // is followed (I. "Handshake" Phase & II. "Data Sync" Phase), and informs the engine
    // accordingly.
    //
    // If the task detects any invalid behaviour from the sync flow it fails critically, indicating
    // that the sync protocol implementation does not behave correctly and is not compatible with
    // the engine.
    //
    // Additionally, the task forwards any synced application data straight to the engine.
    let glue_task_handle: JoinHandle<Result<(), SyncError>> = {
        let engine_actor_tx = engine_actor_tx.clone();
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
                let FromSync::Data { header, payload } = message else {
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

            Ok(())
        })
    };

    // Run the "initiating peer" side of the sync protocol.
    let result = sync_protocol
        .initiate(
            topic.clone(),
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

    // We also return any error originating from the sync protocol implementation itself.
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

    // On a failure we're _not_ sending a `SyncDone` but `SyncFailed` event. This is handled by the
    // sync manager which drives this "initiator" session with additional re-attempt logic.

    engine_actor_tx
        .send(ToEngineActor::SyncDone { peer, topic })
        .await
        .map_err(|err| {
            SyncError::Critical(format!("engine_actor_tx failed sending sync done: {err}"))
        })?;

    Ok(())
}
