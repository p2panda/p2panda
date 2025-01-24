// SPDX-License-Identifier: MIT OR Apache-2.0

use std::sync::Arc;

use anyhow::Result;
use futures_util::{AsyncRead, AsyncWrite, SinkExt};
use p2panda_core::PublicKey;
use p2panda_sync::{FromSync, SyncError, SyncProtocol, TopicQuery};
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;
use tokio_util::sync::PollSender;
use tracing::{debug, error};

use crate::engine::ToEngineActor;

/// Accept a sync protocol session over the provided bi-directional stream for the given peer and
/// topic.
///
/// While this method "drives" the sync protocol implementation it also follows the "2-Phase
/// Protocol Flow" required for the engine to work efficiently. We're expecting the following
/// messages from this "acceptor" flow:
///
/// 1. `SyncStart`: The sync session just began, we don't know the topic yet.
/// 2. `SyncHandshakeSuccess`: We've successfully completed the I. "Handshake" phase, as we've
///    received the topic from the initiator.
/// 3. `SyncMessage` (optional): The actual data we've exchanged with the other peer, this message
///    can occur never or multiple times, depending on how much data was sent.
/// 4. `SyncDone` We've successfully finished this session.
///
/// In case of a detected failure (either through an critical error on our end or an unexpected
/// behaviour from the remote peer), the acceptor will send an `SyncFailed` message instead of the
/// `SyncDone`.
///
/// Errors can be roughly categorized by:
///
/// 1. Critical system failures (bug in p2panda code or sync implementation, sync implementation
///    did not follow "2. Phase Flow" requirements, lack of system resources, etc.)
/// 2. Unexpected Behaviour (remote peer abruptly disconnected, error which got correctly handled
///    in sync implementation, etc.)
pub async fn accept_sync<T, S, R>(
    mut send: &mut S,
    mut recv: &mut R,
    peer: PublicKey,
    sync_protocol: Arc<dyn for<'a> SyncProtocol<'a, T> + 'static>,
    engine_actor_tx: mpsc::Sender<ToEngineActor<T>>,
) -> Result<(), SyncError>
where
    T: TopicQuery + 'static,
    S: AsyncWrite + Send + Unpin,
    R: AsyncRead + Send + Unpin,
{
    debug!("accept sync session with peer {}", peer);

    engine_actor_tx
        .send(ToEngineActor::SyncStart { topic: None, peer })
        .await
        .map_err(|err| {
            SyncError::Critical(format!("engine_actor_tx failed sending sync start: {err}"))
        })?;

    // Set up a channel for receiving messages from the sync session.
    let (tx, mut rx) = mpsc::channel::<FromSync<T>>(128);
    let mut sink = PollSender::new(tx).sink_map_err(|e| SyncError::Critical(e.to_string()));

    // Set up a channel for sending over errors to the "glue" task which occurred during sync.
    let (sync_error_tx, mut sync_error_rx) = oneshot::channel::<SyncError>();

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
    let glue_task_handle: JoinHandle<Result<(), SyncError>> = tokio::spawn(async move {
        let mut topic = None;

        loop {
            tokio::select! {
                biased;

                Ok(err) = &mut sync_error_rx => {
                    engine_actor_tx
                        .send(ToEngineActor::SyncFailed {
                            peer,
                            topic: topic.clone(),
                        })
                        .await
                        .map_err(|err| {
                            SyncError::Critical(
                                format!("engine_actor_tx failed sending sync failed: {err}")
                            )
                        })?;

                    // If we're observing an error we terminate the task here and propagate that
                    // error further up.
                    return Err(err);
                },
                message = rx.recv() => {
                    let Some(message) = message else {
                        // Sink (tx) got dropped, so we're leaving the task.
                        break;
                    };

                    // I. Handshake Phase.
                    //
                    // At the beginning of every sync session the "accepting" peer needs to learn
                    // the topic of the "initiating" peer during the handshake phase. This is
                    // _always_ the first message we're expecting:
                    if let FromSync::HandshakeSuccess(handshake_topic) = message {
                        // It should only be sent once so topic should be `None` now.
                        if topic.is_some() {
                            return Err(
                                SyncError::Critical(
                                    "received topic twice from sync session in handshake phase"
                                    .into()
                                )
                            );
                        }

                        topic = Some(handshake_topic.clone());

                        // Inform the engine that we are expecting sync messages from the peer on
                        // this topic.
                        engine_actor_tx
                            .send(ToEngineActor::SyncHandshakeSuccess {
                                peer,
                                topic: handshake_topic,
                            })
                            .await
                            .map_err(|err| {
                                SyncError::Critical(
                                    format!("engine_actor_tx failed sending handshake success: {err}")
                                )
                            })?;

                        continue;
                    }

                    // II. Data Sync Phase.
                    //
                    // At this stage we're beginning the actual "sync" protocol and expect messages
                    // containing the data which was received from the "initiating" peer.
                    //
                    // Please note that the "accepting" peer does not necessarily receive data in
                    // all sync protocol implementations.
                    //
                    // The topic must be known at this point in order to process further messages.
                    //
                    // Any sync protocol implementation should have already failed with an
                    // "unexpected behaviour" error if the topic wasn't learned. If this didn't
                    // happen (due to an incorrect implementation) we will critically fail now.
                    let Some(topic) = &topic else {
                        return Err(
                            SyncError::Critical(
                                "never received topic from sync session in handshake phase"
                                .into()
                            )
                        );
                    };

                    // From this point on we are only expecting "data" messages from the sync
                    // session.
                    let FromSync::Data { header, payload } = message else {
                        return Err(
                            SyncError::Critical(
                                "expected only data messages from sync session in data sync phase"
                                .into()
                            )
                        );
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
                            SyncError::Critical(
                                format!("engine_actor_tx failed sending sync message: {err}")
                            )
                        })?;
                },
            }
        }

        // If topic was never set then we didn't receive any messages. In that case, the engine
        // wasn't ever informed, so we can return here silently.
        let Some(topic) = topic else {
            return Ok(());
        };

        engine_actor_tx
            .send(ToEngineActor::SyncDone { peer, topic })
            .await
            .map_err(|err| {
                SyncError::Critical(format!("engine_actor_tx failed sending sync done: {err}"))
            })?;

        Ok(())
    });

    // Run the "accepting peer" side of the sync protocol.
    let result = sync_protocol
        .accept(
            Box::new(&mut send),
            Box::new(&mut recv),
            Box::new(&mut sink),
        )
        .await;

    // Drop the tx, so the rx in the glue task receives the closing event.
    drop(sink);

    // The sync protocol failed and we're informing the "glue" task about it, so it can accordingly
    // wind down and inform the engine.
    if let Err(sync_session_err) = result {
        sync_error_tx
            .send(sync_session_err)
            .map_err(|err| SyncError::Critical(format!("sync_error_tx failed: {err}")))?;
    }

    // We're expecting the task to exit with a result soon, we're awaiting it here ..
    let glue_task_result = glue_task_handle
        .await
        .map_err(|err| SyncError::Critical(format!("glue task handle failed: {err}")))?;

    // .. to inform some brave developer who will read the error logs ..
    if let Err(SyncError::Critical(err)) = &glue_task_result {
        error!("critical error in sync protocol: {err}");
    }

    // .. and forward it further!
    glue_task_result
}
