// SPDX-License-Identifier: MIT OR Apache-2.0

use std::error::Error as StdError;
use std::marker::PhantomData;

use futures_channel::mpsc;
use iroh::endpoint::{Connection, VarInt};
use p2panda_sync::topic_handshake::{
    TopicHandshakeEvent, TopicHandshakeInitiator, TopicHandshakeMessage,
};
use p2panda_sync::traits::Protocol;
use ractor::thread_local::ThreadLocalActor;
use ractor::{ActorProcessingErr, ActorRef};
use serde::{Deserialize, Serialize};

use crate::TopicId;
use crate::actors::ActorNamespace;
use crate::actors::iroh::connect;
use crate::actors::sync::SYNC_PROTOCOL_ID;
use crate::addrs::NodeId;
use crate::cbor::{into_cbor_sink, into_cbor_stream};

/// Actor name prefix for a session.
pub const SYNC_SESSION: &str = "net.sync.session";

pub type SyncSessionId = u64;

pub enum SyncSessionMessage<P> {
    Initiate {
        node_id: NodeId,
        topic: TopicId,
        protocol: P,
    },
    Accept {
        connection: Connection,
        protocol: P,
    },
}

pub struct SyncSession<P> {
    _marker: PhantomData<P>,
}

impl<P> Default for SyncSession<P> {
    fn default() -> Self {
        Self {
            _marker: PhantomData,
        }
    }
}

impl<P> ThreadLocalActor for SyncSession<P>
where
    P: Protocol + Send + 'static,
    P::Error: StdError + Send + Sync + 'static,
    for<'a> P::Message: Serialize + Deserialize<'a>,
{
    type State = ActorNamespace;

    type Msg = SyncSessionMessage<P>;

    type Arguments = ActorNamespace;

    async fn pre_start(
        &self,
        _myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        Ok(args)
    }

    async fn handle(
        &self,
        _myself: ActorRef<Self::Msg>,
        message: Self::Msg,
        actor_namespace: &mut Self::State,
    ) -> Result<(), ActorProcessingErr> {
        match message {
            SyncSessionMessage::Initiate {
                node_id,
                topic,
                protocol,
            } => {
                let connection =
                    connect(node_id, SYNC_PROTOCOL_ID, actor_namespace.to_string()).await?;

                // First we run the TopicHandshake protocol.
                let (tx, rx) = connection.open_bi().await?;
                let mut tx = into_cbor_sink::<TopicHandshakeMessage<TopicId>, _>(tx);
                let mut rx = into_cbor_stream::<TopicHandshakeMessage<TopicId>, _>(rx);

                // @NOTE: We don't need to observe these events here as the topic is returned as output
                // when the protocol completes, so these channels are actually only just to satisfy the
                // API.
                let (event_tx, _event_rx) = mpsc::channel::<TopicHandshakeEvent<TopicId>>(128);
                let topic_handshake = TopicHandshakeInitiator::new(topic, event_tx);
                topic_handshake.run(&mut tx, &mut rx).await?;

                // Then we run the actual sync protocol.
                let (tx, rx) = connection.open_bi().await?;
                let mut tx = into_cbor_sink::<P::Message, _>(tx);
                let mut rx = into_cbor_stream::<P::Message, _>(rx);
                protocol.run(&mut tx, &mut rx).await?;

                // @NOTE: in order to ensure all sent messages can be received and processed by
                // both peers, sync protocol implementations must coordinate the close of a
                // connection. Normally this would mean one side sends a "last message" and then
                // waits for the other to close the connection themselves. If this doesn't occur
                // in a timely manner then the connection will timeout.
                connection.close(VarInt::from_u32(0), b"sync protocol initiate completed");
            }
            SyncSessionMessage::Accept {
                connection,
                protocol,
            } => {
                // @NOTE: the TopicHandshake protocol has already been run by the accepting party
                // which is why we don't perform that additional step here.
                let (tx, rx) = connection.accept_bi().await?;
                let mut tx = into_cbor_sink::<P::Message, _>(tx);
                let mut rx = into_cbor_stream::<P::Message, _>(rx);
                protocol.run(&mut tx, &mut rx).await?;

                // @NOTE: see comment above regarding graceful connection closure.
                connection.close(VarInt::from_u32(0), b"sync protocol accept completed");
            }
        }
        Ok(())
    }
}
