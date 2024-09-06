// SPDX-License-Identifier: AGPL-3.0-or-later

use futures::{AsyncRead, AsyncWrite, Sink, Stream};

use crate::{engine::Session, SyncError};

#[trait_variant::make(SyncProtocol: Send)]
pub trait LocalSyncProtocol {
    type Topic;
    type Message;
    type Context;

    async fn run(
        self,
        topic: Self::Topic,
        sink: impl Sink<Self::Message, Error = SyncError> + Send + Unpin,
        stream: impl Stream<Item = Result<Self::Message, SyncError>> + Send + Unpin,
        context: Self::Context,
    ) -> Result<(), SyncError>;
}

pub trait SyncEngine<P, TX, RX>
where
    P: SyncProtocol,
    TX: AsyncWrite,
    RX: AsyncRead,
{
    type Sink: Sink<<P as SyncProtocol>::Message, Error = SyncError> + Send + Unpin;
    type Stream: Stream<Item = Result<<P as SyncProtocol>::Message, SyncError>> + Send + Unpin;

    fn session(protocol: P, tx: TX, rx: RX) -> Session<P, Self::Sink, Self::Stream>;
}
