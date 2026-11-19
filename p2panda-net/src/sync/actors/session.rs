// SPDX-License-Identifier: MIT OR Apache-2.0

use std::marker::PhantomData;

use iroh::endpoint::{Connection, RecvStream, SendStream, VarInt};
use p2panda_core::Topic;
use p2panda_sync::protocols::{
    TopicHandshakeEvent, TopicHandshakeInitiator, TopicHandshakeMessage,
};
use p2panda_sync::traits::Protocol;
use p2panda_sync::{FromSync, ToSync};
use ractor::thread_local::ThreadLocalActor;
use ractor::{ActorProcessingErr, ActorRef};
use tokio::sync::mpsc;
use tracing::{Instrument, warn};

use crate::codec::{into_codec_sink, into_codec_stream};
use crate::iroh_endpoint::Endpoint;
use crate::sync::actors::ToTopicManager;
use crate::sync::session::SessionBuilder;
use crate::utils::ShortFormat;
use crate::{NodeId, ProtocolId};

/// Capacity of per-session event channel.
const EVENT_BUFFER_SIZE: usize = 1024;

pub type SyncSessionId = u64;

pub enum SyncSessionMessage<S>
where
    S: SessionBuilder,
{
    Initiate {
        protocol_id: ProtocolId,
        live_rx: Option<mpsc::Receiver<ToSync<S::Operation>>>,
    },
    Accept {
        connection: Connection,
        live_rx: Option<mpsc::Receiver<ToSync<S::Operation>>>,
    },
}

pub struct SyncSession<S> {
    _marker: PhantomData<S>,
}

impl<S> Default for SyncSession<S> {
    fn default() -> Self {
        Self {
            _marker: PhantomData,
        }
    }
}

pub struct SessionState<S>
where
    S: SessionBuilder,
{
    pub endpoint: Endpoint,
    pub session_builder: S,
    pub topic: Topic,
    pub remote: NodeId,
    pub session_id: SyncSessionId,
    pub parent_ref: ActorRef<ToTopicManager<S::Operation, S::Event>>,
}

impl<S> ThreadLocalActor for SyncSession<S>
where
    S: SessionBuilder,
{
    type State = SessionState<S>;

    type Msg = SyncSessionMessage<S>;

    type Arguments = SessionState<S>;

    async fn pre_start(
        &self,
        _myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        Ok(args)
    }

    async fn handle(
        &self,
        myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        state: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            SyncSessionMessage::Initiate {
                protocol_id,
                live_rx,
            } => {
                let connection = state.endpoint.connect(state.remote, protocol_id).await?;

                // First run the TopicHandshake protocol.
                let (tx, rx) = connection.open_bi().await?;
                let mut tx = into_codec_sink::<TopicHandshakeMessage<Topic>, _>(tx);
                let mut rx = into_codec_stream::<TopicHandshakeMessage<Topic>, _>(rx);

                {
                    // We don't need to observe these events here as the topic is returned as
                    // output when the protocol completes, so these channels are actually only
                    // just to satisfy the API.
                    let (event_tx, _event_rx) = mpsc::channel::<TopicHandshakeEvent<Topic>>(128);
                    let topic_handshake = TopicHandshakeInitiator::new(state.topic, event_tx);
                    topic_handshake.run(&mut tx, &mut rx).await?;
                }

                // Open another connection with the remote and run the sync protocol.
                let (tx, rx) = connection.open_bi().await?;

                let span = tracing::debug_span!("sync", responder = %state.remote.fmt_short(), topic = %state.topic.fmt_short(), state.session_id);
                Self::sync(state, tx, rx, live_rx).instrument(span).await?;

                // In order to ensure all sent messages can be received and processed by both
                // peers, sync protocol implementations must coordinate the close of a connection.
                // Normally this would mean one side sends a "last message" and then waits for the
                // other to close the connection themselves. If this doesn't occur in a timely
                // manner then the connection will timeout.
                connection.close(VarInt::from_u32(0), b"sync protocol initiate completed");
            }
            SyncSessionMessage::Accept {
                connection,
                live_rx,
            } => {
                // The TopicHandshake protocol has already been run by the accepting party which
                // is why we don't perform that additional step here.
                let (tx, rx) = connection.accept_bi().await?;
                let span = tracing::debug_span!("sync", requester = %state.remote.fmt_short(), topic = %state.topic.fmt_short(), state.session_id);
                Self::sync(state, tx, rx, live_rx).instrument(span).await?;

                // See comment above regarding graceful connection closure.
                connection.close(VarInt::from_u32(0), b"sync protocol accept completed");
            }
        }

        // Gracefully close the actor when the sync session is complete.
        //
        // Note the actor will only ever process one message ("initiate" or "accept").
        myself.stop(Some("sync session shutting down".into()));

        Ok(())
    }
}

impl<S> SyncSession<S>
where
    S: SessionBuilder,
{
    /// Run a sync session with the remote.
    async fn sync(
        state: &SessionState<S>,
        tx: SendStream,
        rx: RecvStream,
        live_rx: Option<mpsc::Receiver<ToSync<S::Operation>>>,
    ) -> Result<(), ActorProcessingErr> {
        let SessionState {
            session_builder,
            topic,
            remote,
            session_id,
            ..
        } = state;

        let session_id = *session_id;
        let remote = *remote;
        let parent_ref = state.parent_ref.clone();

        let mut tx = into_codec_sink(tx);
        let mut rx = into_codec_stream(rx);

        // TODO: This should be a mpsc channel.
        let (event_tx, mut event_rx) = mpsc::channel(EVENT_BUFFER_SIZE);
        let protocol = session_builder.build(*topic, live_rx, event_tx);

        // Spawn task for forwarding session events to back to the TopicManager where they will be
        // de-duplicated and forwarded to _other_ sync sessions and subscribers.
        let handle = tokio::task::spawn(async move {
            loop {
                let Some(event) = event_rx.recv().await else {
                    // Stream closes when the sync session finishes.
                    break;
                };

                let event = ToTopicManager::SessionItem(FromSync {
                    session_id,
                    remote,
                    event,
                });

                if let Err(err) = parent_ref.send_message(event) {
                    warn!("failed sending to parent inbox: {err:?}");
                    break;
                }
            }
        });

        // Run the sync protocol.
        let result = protocol.run(&mut tx, &mut rx).await;

        // Await the forwarder task completing, this will exit after the sync session completes.
        let _ = handle.await;

        result?;

        Ok(())
    }
}
