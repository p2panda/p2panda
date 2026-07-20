// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fmt::Debug;
use std::marker::PhantomData;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use futures_util::stream::BoxStream;
use futures_util::{FutureExt, Stream};
use p2panda_core::cbor::{EncodeError, encode_cbor};
use p2panda_core::{Hash, Topic};
use p2panda_net::sync::SyncHandle;
use p2panda_net::utils::ShortFormat;
use p2panda_sync::protocols::TopicLogSyncEvent;
use serde::Serialize;
use thiserror::Error;
use tokio::sync::{mpsc, oneshot};

use crate::forge::{Forge, ForgeError, OperationForge};
use crate::operation::{Extensions, LogId, Operation};
use crate::spaces::{RepairError, RepairStrategy};
use crate::streams::drop_guard::StreamDropGuard;
use crate::streams::external_stream::ExternalStreamFuture;
use crate::streams::local_stream::LocalStreamFuture;
use crate::streams::{Event, StreamEvent};

type PublishTx<M> = mpsc::Sender<(Operation, Option<M>, oneshot::Sender<Event>)>;
type ImportExternalTx = mpsc::Sender<(
    BoxStream<'static, Operation>,
    oneshot::Sender<ExternalStreamFuture>,
)>;
type ImportLocalTx = mpsc::Sender<(
    BoxStream<'static, Operation>,
    oneshot::Sender<LocalStreamFuture>,
)>;
type ToOutputTx<M> = mpsc::Sender<Vec<StreamEvent<M>>>;
type RepairTx = mpsc::Sender<(RepairStrategy, oneshot::Sender<Result<bool, RepairError>>)>;

/// Publish messages into a topic stream.
///
/// Any message type `M` can be published as long as it can be encoded into bytes by implementing
/// serde's [`Serialize`] and [`Deserialize`] traits.
///
/// ## Example
///
/// ```rust
/// # use p2panda_core::Topic;
/// # use serde::{Serialize, Deserialize};
/// # #[tokio::main]
/// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
/// # let node = p2panda::builder().spawn().await?;
/// #
/// let our_trees = Topic::random();
///
/// #[derive(Clone, Debug, Serialize, Deserialize)]
/// struct Tree {
///     leafy: bool,
///     latin_name: String,
/// }
///
/// let (tx, _rx) = node.stream::<Tree>(our_trees).await?;
///
/// tx.publish(Tree {
///     leafy: true,
///     latin_name: "Acer pseudoplatanus".into(),
/// }).await?;
/// #
/// # Ok(())
/// # }
/// ```
///
/// ## Append-only log
///
/// The Node API internally maintains an append-only log data-type for published application
/// messages in a topic stream.
///
/// Publishing a message creates and signs an [`Operation`] which gets automatically appended to
/// the author's log for the given topic. The message itself is the payload of the created
/// operation.
///
/// ```plain
/// Author "Panda"
/// Topic "Trees" Log: [ Header ] <-- [ Header ] <-- [ Header ] ...
///                         |             |              |
///                         v            ...            ...
///                     [ Body ]
///               "Acer pseudoplatanus"
///
/// Author "Icebear"
/// Topic "Trees" Log: [ Header ] <-- [ Header ] ...
///                         |             |
///                         v            ...
///                     [ Body ]
///                 "Pinus halepensis"
/// ```
///
/// ## External sources
///
/// Operations can be imported from external sources into the processing pipeline by calling
/// [`StreamPublisher::import`]. This allows transporting data via sneakernets (USB stick, etc.) or
/// other sync solutions.
#[derive(Clone, Debug)]
pub struct StreamPublisher<M> {
    topic: Topic,
    forge: OperationForge,
    sync_handle: Arc<SyncHandle<Operation, TopicLogSyncEvent<Extensions>>>,
    #[allow(clippy::type_complexity)]
    pub(crate) publish_tx: PublishTx<M>,
    import_external_tx: ImportExternalTx,
    #[allow(clippy::type_complexity)]
    import_local_tx: ImportLocalTx,
    pub(crate) to_output_tx: ToOutputTx<M>,
    pub(crate) repair_tx: RepairTx,
    _guard: StreamDropGuard,
    _marker: PhantomData<M>,
}

impl<M> StreamPublisher<M>
where
    M: Serialize,
{
    /// Create a new `StreamPublisher`.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        topic: Topic,
        forge: OperationForge,
        sync_handle: Arc<SyncHandle<Operation, TopicLogSyncEvent<Extensions>>>,
        publish_tx: PublishTx<M>,
        import_external_tx: ImportExternalTx,
        import_local_tx: ImportLocalTx,
        repair_tx: RepairTx,
        to_output_tx: ToOutputTx<M>,
        _guard: StreamDropGuard,
    ) -> Self {
        Self {
            topic,
            forge,
            sync_handle,
            publish_tx,
            import_external_tx,
            import_local_tx,
            repair_tx,
            to_output_tx,
            _guard,
            _marker: PhantomData,
        }
    }

    /// Associated topic.
    pub fn topic(&self) -> Topic {
        self.topic
    }

    /// Publish a message into a topic stream.
    ///
    /// Locally created operations are processed by the same pipeline as remotely received
    /// operations. It is possible to await the processing result which can be useful for some
    /// applications if they want to block UI components etc.
    pub async fn publish(&self, message: M) -> Result<PublishFuture, PublishError> {
        self.publish_inner(Some(message), false).await
    }

    async fn publish_inner(
        &self,
        message: Option<M>,
        prune_flag: bool,
    ) -> Result<PublishFuture, PublishError> {
        // Create, sign and persist operation with given payload.
        let extensions = Extensions::builder(LogId::from_topic(self.topic()))
            .prune_flag(prune_flag)
            .build();

        let body_bytes = match message {
            Some(ref message) => Some(encode_cbor(&message)?),
            None => None,
        };

        let operation = self
            .forge
            .create_operation(
                Some(self.topic()),
                extensions.log_id(),
                body_bytes,
                extensions,
            )
            .await?;
        let hash = operation.hash;

        // Start processing operation in pipeline. Keep a oneshot receiver around to allow users to
        // optionally await & get informed when processing has finished.
        let (processed_tx, processed_rx) = oneshot::channel();
        self.publish_tx
            .send((operation.clone(), message, processed_tx))
            .await
            .map_err(|err| PublishError::SendToProcessor(err.to_string()))?;

        Ok(PublishFuture { hash, processed_rx })
    }

    /// Deletes all our previously published messages in this topic stream.
    ///
    /// This signals to all other nodes that they should remove them as well.
    ///
    /// A message can be optionally added when pruning, allowing to publish a "snapshot" /
    /// state-based CRDT of the current state, so nodes can still consistently re-create all state,
    /// even if previous messages are gone.
    ///
    /// Internally we're applying append-only log prefix deletion, meaning that the log's prefix
    /// gets pruned. The prefix is the set of operations in the log's sequence which are causally
    /// "older" / before the point where the prune flag was set.
    pub async fn prune(&self, message: Option<M>) -> Result<PublishFuture, PublishError> {
        self.publish_inner(message, true).await
    }

    /// Import an external source of operations.
    ///
    /// Please note: Operations do not contain any information by themselves about to which topic
    /// they belong. By importing operations into a topic stream, they will be assigned to this
    /// topic. Make sure you accordingly routed operations into the correct topic before.
    pub async fn import(
        &self,
        stream: impl Stream<Item = Operation> + Send + 'static,
    ) -> Result<ExternalStreamFuture, ImportError> {
        // Send stream to processor.
        let stream = Box::pin(stream);
        let (ready_tx, ready_rx) = oneshot::channel::<ExternalStreamFuture>();
        self.import_external_tx
            .send((stream, ready_tx))
            .await
            .map_err(|err| ImportError::SendToProcessor(err.to_string()))?;

        // Await receiving the session id and future which will complete when the external stream
        // closes and all operations have been processed.
        ready_rx
            .await
            .map_err(|err| ImportError::ReceiveFromProcessor(err.to_string()))
    }

    /// Import a local source of operations.
    ///
    /// This method can be used for importing some locally forged operations in a batch. The user
    /// will receive no "import" events on the stream, only events resulting from publishing the
    /// operations themselves.
    pub(crate) async fn import_local(
        &self,
        stream: impl Stream<Item = Operation> + Send + 'static,
    ) -> Result<LocalStreamFuture, ImportError> {
        // Send stream to processor.
        let stream = Box::pin(stream);
        let (ready_tx, ready_rx) = oneshot::channel::<LocalStreamFuture>();
        self.import_local_tx
            .send((stream, ready_tx))
            .await
            .map_err(|err| ImportError::SendToProcessor(err.to_string()))?;

        // Await receiving the future which will complete when the stream
        // closes and all operations have been processed.
        ready_rx
            .await
            .map_err(|err| ImportError::ReceiveFromProcessor(err.to_string()))
    }

    /// Close the associated sync session gracefully.
    ///
    /// This method can be awaited to ensure that all sync-related state has been cleaned up
    /// internally.
    ///
    /// Call `close()` if you wish to drop a `StreamPublisher` and then immediately create a new
    /// stream for the same topic.
    pub async fn close(&self) -> Result<(), CloseError> {
        self.sync_handle
            .close()
            .await
            .map_err(|_| CloseError(self.topic.fmt_short()))?;

        Ok(())
    }
}

#[derive(Debug, Error)]
#[error("an error occurred while closing a stream publisher sync handle for topic {0}")]
pub struct CloseError(String);

/// Future which can be awaited to find out when a locally published operation has finished
/// processing.
#[derive(Debug)]
pub struct PublishFuture {
    hash: Hash,
    processed_rx: oneshot::Receiver<Event>,
}

impl PublishFuture {
    /// Returns hash of the published operation.
    pub fn hash(&self) -> Hash {
        self.hash
    }
}

impl Future for PublishFuture {
    type Output = Result<Event, oneshot::error::RecvError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.processed_rx.poll_unpin(cx)
    }
}

/// Error occurred when creating operation and publishing it to topic stream.
#[derive(Debug, Error)]
pub enum PublishError {
    #[error("an error occurred while serializing the message for publication: {0}")]
    MessageEncoding(#[from] EncodeError),

    #[error("an error occurred while creating an operation in the forge: {0}")]
    Forge(#[from] ForgeError),

    #[error("an error occurred while publishing an operation to the log sync stream: {0}")]
    SyncHandle(String),

    #[error("could not send to processor pipeline: {0}")]
    SendToProcessor(String),
}

#[derive(Debug, Error)]
pub enum ImportError {
    #[error("could not send to processor pipeline: {0}")]
    SendToProcessor(String),

    #[error("an error occurred awaiting message from processor: {0}")]
    ReceiveFromProcessor(String),
}
