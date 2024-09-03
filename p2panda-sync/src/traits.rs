// SPDX-License-Identifier: AGPL-3.0-or-later

use futures::{AsyncRead, AsyncWrite, Sink, Stream};
use thiserror::Error;

use crate::engine::Session;

#[derive(Error, Debug)]
pub enum SyncError {
    #[error("protocol error: {0}")]
    Protocol(String),
    #[error("input/output error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("codec error: {0}")]
    Codec(String),
    #[error("custom error: {0}")]
    Custom(String),
}

#[trait_variant::make(SyncProtocol: Send)]
pub trait LocalSyncProtocol {
    type Topic;
    type Message;

    async fn run(
        self,
        topic: Self::Topic,
        sink: impl Sink<Self::Message, Error = SyncError> + Send + Unpin,
        stream: impl Stream<Item = Result<Self::Message, SyncError>> + Send + Unpin,
    ) -> Result<(), SyncError>;
}

pub trait SyncEngine<P, TX, RX>
where
    P: SyncProtocol,
    TX: AsyncWrite,
    RX: AsyncRead,
{
    type Sink: Sink<<P as SyncProtocol>::Message, Error = SyncError>;
    type Stream: Stream<Item = Result<<P as SyncProtocol>::Message, SyncError>>;

    fn session(&self, tx: TX, rx: RX) -> Session<P, Self::Sink, Self::Stream>;
}
