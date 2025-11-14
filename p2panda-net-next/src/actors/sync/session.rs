// SPDX-License-Identifier: MIT OR Apache-2.0

use std::error::Error as StdError;
use std::fmt::Debug;
use std::hash::Hash as StdHash;
use std::marker::PhantomData;

use futures_channel::mpsc;
use iroh::endpoint::Connection;
use p2panda_sync::topic_handshake::{
    TopicHandshakeEvent, TopicHandshakeInitiator, TopicHandshakeMessage,
};
use p2panda_sync::traits::Protocol;
use ractor::thread_local::ThreadLocalActor;
use ractor::{Actor, ActorProcessingErr, ActorRef};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncWrite};

use crate::actors::ActorNamespace;
use crate::actors::iroh::connect;
use crate::actors::sync::SYNC_PROTOCOL_ID;
use crate::addrs::{NodeId, NodeInfo};
use crate::args::ApplicationArguments;
use crate::cbor::{CborCodec, into_cbor_sink, into_cbor_stream};

pub enum SyncSessionMessage<T, P> {
    Initiate {
        node_id: NodeId,
        topic: T,
        protocol: P,
    },
    Accept {
        connection: Connection,
        protocol: P,
    },
}

pub struct SyncSession<T, P> {
    _marker: PhantomData<(T, P)>,
}

impl<T, P> Default for SyncSession<T, P> {
    fn default() -> Self {
        Self {
            _marker: PhantomData,
        }
    }
}

impl<T, P> ThreadLocalActor for SyncSession<T, P>
where
    for<'a> T: Clone + Debug + StdHash + Eq + Send + Sync + Serialize + Deserialize<'a> + 'static,
    P: Protocol + Send + 'static,
    P::Error: StdError + Send + Sync + 'static,
    for<'a> P::Message: Serialize + Deserialize<'a>,
{
    type State = ActorNamespace;

    type Msg = SyncSessionMessage<T, P>;

    type Arguments = ActorNamespace;

    async fn pre_start(
        &self,
        myself: ActorRef<Self::Msg>,
        args: Self::Arguments,
    ) -> Result<Self::State, ActorProcessingErr> {
        Ok(args)
    }

    async fn handle(
        &self,
        myself: ActorRef<Self::Msg>,
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
                    connect::<T>(node_id, SYNC_PROTOCOL_ID, actor_namespace.to_string()).await?;

                // First we run the TopicHandshake protocol.
                let (tx, rx) = connection.open_bi().await?;
                let mut tx = into_cbor_sink::<TopicHandshakeMessage<T>, _>(tx);
                let mut rx = into_cbor_stream::<TopicHandshakeMessage<T>, _>(rx);

                // @NOTE: We don't need to observe these events here as the topic is returned as output
                // when the protocol completes, so these channels are actually only just to satisfy the
                // API.
                let (event_tx, _event_rx) = mpsc::channel::<TopicHandshakeEvent<T>>(128);
                let topic_handshake = TopicHandshakeInitiator::new(topic, event_tx);
                topic_handshake.run(&mut tx, &mut rx).await?;

                // Then we run the actual sync protocol.
                let (tx, rx) = connection.open_bi().await?;
                let mut tx = into_cbor_sink::<P::Message, _>(tx);
                let mut rx = into_cbor_stream::<P::Message, _>(rx);
                protocol.run(&mut tx, &mut rx).await?;
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
            }
        }
        Ok(())
    }
}
