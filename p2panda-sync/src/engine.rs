// SPDX-License-Identifier: AGPL-3.0-or-later

use futures::{AsyncRead, AsyncWrite, Sink, Stream};
use serde::de::DeserializeOwned;
use serde::Serialize;
use tokio_util::codec::{FramedRead, FramedWrite};
use tokio_util::compat::{Compat, FuturesAsyncReadCompatExt, FuturesAsyncWriteCompatExt};

use crate::codec::CborCodec;
use crate::traits::{SyncEngine, SyncError, SyncProtocol, SyncSession};

pub struct Engine<P> {
    pub protocol: P,
}

pub struct Session<P, SI, ST> {
    protocol: P,
    sink: SI,
    stream: ST,
}

impl<P, SI, ST> SyncSession<P, SI, ST> for Session<P, SI, ST>
where
    <P as SyncProtocol>::Topic: Send,
    P: SyncProtocol + Send,
    SI: Sink<<P as SyncProtocol>::Message, Error = SyncError> + Send + Unpin,
    ST: Stream<Item = Result<<P as SyncProtocol>::Message, SyncError>> + Send + Unpin,
{
    async fn run(self, topic: <P as SyncProtocol>::Topic) -> Result<(), SyncError> {
        self.protocol.run(topic, self.sink, self.stream).await
    }
}

type EngineSink<TX, M> = FramedWrite<Compat<TX>, CborCodec<M>>;
type EngineStream<RX, M> = FramedRead<Compat<RX>, CborCodec<M>>;

impl<P, TX, RX> SyncEngine<P, TX, RX> for Engine<P>
where
    <P as SyncProtocol>::Topic: Send,
    <P as SyncProtocol>::Message: Serialize + DeserializeOwned + Send,
    P: Clone + SyncProtocol,
    TX: AsyncWrite + Send + Unpin,
    RX: AsyncRead + Send + Unpin,
{
    type Sink = EngineSink<TX, <P as SyncProtocol>::Message>;
    type Stream = EngineStream<RX, <P as SyncProtocol>::Message>;
    type Session = Session<P, Self::Sink, Self::Stream>;

    fn new(protocol: P) -> Engine<P> {
        Engine { protocol }
    }

    fn session(&self, tx: TX, rx: RX) -> Self::Session {
        // Convert the `AsyncRead` and `AsyncWrite` into framed (typed) `Stream` and `Sink`. We provide a custom
        // `tokio_util::codec::Decoder` and `tokio_util::codec::Encoder` for this purpose.
        let codec = CborCodec::<<P as SyncProtocol>::Message>::new();
        let sink = FramedWrite::new(tx.compat_write(), codec.clone());
        let stream = FramedRead::new(rx.compat(), codec);

        Session {
            protocol: self.protocol.clone(),
            stream,
            sink,
        }
    }
}
