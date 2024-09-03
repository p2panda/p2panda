use futures::{AsyncRead, AsyncWrite, Sink, Stream};
use thiserror::Error;

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
    type Message: Clone;

    async fn run(
        self,
        topic: Self::Topic,
        sink: impl Sink<Self::Message, Error = SyncError> + Send + Unpin,
        stream: impl Stream<Item = Result<Self::Message, SyncError>> + Send + Unpin,
    ) -> Result<(), SyncError>;
}

#[trait_variant::make(SyncSession: Send)]
pub trait LocalSyncSession<P, SI, ST>
where
    P: SyncProtocol,
    SI: Sink<<P as SyncProtocol>::Message, Error = SyncError>,
    ST: Stream<Item = Result<<P as SyncProtocol>::Message, SyncError>>,
{
    async fn run(self, topic: <P as SyncProtocol>::Topic) -> Result<(), SyncError>;
}

pub trait SyncEngine<P, TX, RX>
where
    P: SyncProtocol,
    TX: AsyncWrite,
    RX: AsyncRead,
{
    type Sink: Sink<<P as SyncProtocol>::Message, Error = SyncError>;
    type Stream: Stream<Item = Result<<P as SyncProtocol>::Message, SyncError>>;
    type Session: SyncSession<P, Self::Sink, Self::Stream>;

    fn new(strategy: P) -> Self;

    fn session(&self, tx: TX, rx: RX) -> Self::Session;
}
