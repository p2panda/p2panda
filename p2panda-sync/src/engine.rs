// SPDX-License-Identifier: AGPL-3.0-or-later

use futures::{AsyncRead, AsyncWrite, Sink, Stream};
use serde::{Deserialize, Serialize};
use tokio_util::codec::{FramedRead, FramedWrite};
use tokio_util::compat::{Compat, FuturesAsyncReadCompatExt, FuturesAsyncWriteCompatExt};

use crate::codec::CborCodec;
use crate::traits::{SyncEngine, SyncProtocol};
use crate::SyncError;

pub struct Engine<P> {
    pub protocol: P,
}

impl<P> Engine<P>
where
    P: SyncProtocol,
{
    pub fn new(protocol: P) -> Self {
        Engine { protocol }
    }
}

pub struct Session<P, SI, ST> {
    protocol: P,
    sink: SI,
    stream: ST,
}

impl<P, SI, ST> Session<P, SI, ST>
where
    <P as SyncProtocol>::Topic: Send,
    P: SyncProtocol + Send,
    SI: Sink<<P as SyncProtocol>::Message, Error = SyncError> + Send + Unpin,
    ST: Stream<Item = Result<<P as SyncProtocol>::Message, SyncError>> + Send + Unpin,
{
    pub async fn run(
        self,
        topic: <P as SyncProtocol>::Topic,
        context: <P as SyncProtocol>::Context,
    ) -> Result<(), SyncError>
    {
        self.protocol
            .run(topic, self.sink, self.stream, context)
            .await
    }
}

type EngineSink<TX, M> = FramedWrite<Compat<TX>, CborCodec<M>>;
type EngineStream<RX, M> = FramedRead<Compat<RX>, CborCodec<M>>;

impl<P, TX, RX> SyncEngine<P, TX, RX> for Engine<P>
where
    <P as SyncProtocol>::Topic: Send,
    for<'de> <P as SyncProtocol>::Message: Serialize + Send + Deserialize<'de>,
    P: Clone + SyncProtocol,
    TX: AsyncWrite + Send + Unpin,
    RX: AsyncRead + Send + Unpin,
{
    type Sink = EngineSink<TX, <P as SyncProtocol>::Message>;
    type Stream = EngineStream<RX, <P as SyncProtocol>::Message>;

    fn session(protocol: P, tx: TX, rx: RX) -> Session<P, Self::Sink, Self::Stream> {
        // Convert the `AsyncRead` and `AsyncWrite` into framed (typed) `Stream` and `Sink`. We provide a custom
        // `tokio_util::codec::Decoder` and `tokio_util::codec::Encoder` for this purpose.
        let sink = FramedWrite::new(
            tx.compat_write(),
            CborCodec::<<P as SyncProtocol>::Message>::new(),
        );
        let stream = FramedRead::new(
            rx.compat(),
            CborCodec::<<P as SyncProtocol>::Message>::new(),
        );

        Session {
            protocol,
            stream,
            sink,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::sync::{Arc, RwLock};

    use futures::{Sink, SinkExt, Stream, StreamExt};
    use serde::{Deserialize, Serialize};
    use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

    use crate::traits::{SyncEngine, SyncProtocol};
    use crate::{Engine, SyncError};

    #[tokio::test]
    async fn protocol_impl() {
        // The topic (can represent a sub-set of all items) we are performing sync over.
        const TOPIC_ID: &str = "all_animals";

        // The protocol message types.
        #[derive(Serialize, Deserialize)]
        enum Message {
            Have(HashSet<String>),
            Take(HashSet<String>),
        }

        // Protocol struct and implementation of `SyncProtocol` trait.
        #[derive(Clone)]
        struct MyProtocol {
            set: Arc<RwLock<HashSet<String>>>,
        }

        // A very naive sync protocol.
        impl SyncProtocol for MyProtocol {
            type Topic = &'static str;
            type Message = Message;
            type Context = ();

            async fn run(
                self,
                topic: Self::Topic,
                mut sink: impl Sink<Message, Error = SyncError> + Unpin,
                mut stream: impl Stream<Item = Result<Message, SyncError>> + Unpin,
                context: Self::Context,
            ) -> Result<(), SyncError> {
                if topic != TOPIC_ID {
                    return Err(SyncError::Protocol("not my animal topic".to_string()));
                }
                let local_set = self.set.read().unwrap().clone();
                sink.send(Message::Have(local_set.clone())).await?;

                while let Some(result) = stream.next().await {
                    let message = result?;

                    match message {
                        Message::Have(remote_set) => {
                            let remote_needs = local_set.difference(&remote_set).cloned().collect();
                            sink.send(Message::Take(remote_needs)).await?;
                        }
                        Message::Take(from_remote) => {
                            self.set.write().unwrap().extend(from_remote.into_iter());
                            break;
                        }
                    }
                }

                Ok(())
            }
        }

        // Construct a sync engine for peer a and b.
        let peer_a_set =
            HashSet::from(["Cat".to_string(), "Dog".to_string(), "Rabbit".to_string()]);
        let peer_a_set = Arc::new(RwLock::new(peer_a_set));

        let peer_b_set = HashSet::from([
            "Cat".to_string(),
            "Penguin".to_string(),
            "Panda".to_string(),
        ]);
        let peer_b_set = Arc::new(RwLock::new(peer_b_set));

        // Create a duplex stream which simulate both ends of a bi-directional network connection.
        let (peer_a, peer_b) = tokio::io::duplex(64 * 1024);
        let (peer_a_read, peer_a_write) = tokio::io::split(peer_a);
        let (peer_b_read, peer_b_write) = tokio::io::split(peer_b);

        // Create and spawn a task for running sync sessions for peer a and peer b.
        let peer_protocol_a = MyProtocol {
            set: peer_a_set.clone(),
        };
        let peer_a_session = Engine::session(
            peer_protocol_a,
            peer_a_write.compat_write(),
            peer_a_read.compat(),
        );
        let handle1 = tokio::spawn(async move {
            let _ = peer_a_session.run(TOPIC_ID, ()).await.unwrap();
        });

        let peer_b_protocol = MyProtocol {
            set: peer_b_set.clone(),
        };
        let peer_b_session = Engine::session(
            peer_b_protocol,
            peer_b_write.compat_write(),
            peer_b_read.compat(),
        );
        let handle2 = tokio::spawn(async move {
            let _ = peer_b_session.run(TOPIC_ID, ()).await.unwrap();
        });

        // Wait for both sessions to complete.
        let _ = tokio::join!(handle1, handle2);

        // Both peers' sets now contain the same items.
        let peer_a_set = peer_a_set.read().unwrap().clone();
        let peer_b_set = peer_b_set.read().unwrap().clone();
        assert_eq!(peer_a_set, peer_b_set);
    }
}
