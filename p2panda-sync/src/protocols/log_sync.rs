// SPDX-License-Identifier: MIT OR Apache-2.0

//! Two-party sync protocol over append-only logs.
use std::collections::BTreeMap;
use std::fmt::{Debug, Display};
use std::marker::PhantomData;

use futures::{Sink, SinkExt, Stream, StreamExt, stream};
use p2panda_core::cbor::{DecodeError, decode_cbor};
use p2panda_core::logs::{LogHeights, LogRanges, compare};
use p2panda_core::{Body, Extensions, Hash, Header, LogId, Operation, PublicKey};
use p2panda_store::logs::LogStore;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::select;
use tokio::sync::broadcast;
use tracing::{debug, trace, warn};

use crate::dedup::{DEFAULT_BUFFER_CAPACITY, Dedup};
use crate::traits::Protocol;

/// A map of author logs.
pub type Logs<L> = BTreeMap<PublicKey, Vec<L>>;

/// Sync session life-cycle states.
#[derive(Debug, Default)]
enum State<L> {
    /// Initialise session metrics and announce sync start on event stream.
    #[default]
    Start,

    /// Calculate local log heights and send Have message to remote.
    SendHave,

    /// Receive have message from remote and calculate operation diff.
    ReceiveHave { local: LogHeights<PublicKey, L> },

    /// Send PreSync message to remote or Done if we have nothing to send.
    SendPreSync {
        remote_needs: LogRanges<PublicKey, L>,
    },

    /// Receive PreSync message from remote or Done if they have nothing to send.
    ReceivePreSyncOrDone {
        remote_needs: LogRanges<PublicKey, L>,
        outbound_operations: u64,
        outbound_bytes: u64,
    },

    /// Enter sync loop where we exchange operations with the remote, moves onto next state when
    /// both peers have send Done messages.
    Sync {
        remote_needs: LogRanges<PublicKey, L>,
        metrics: LogSyncMetrics,
    },

    /// Announce on the event stream that the sync session successfully completed.
    End { metrics: LogSyncMetrics },
}

/// Efficient sync protocol for append-only log data types.
#[derive(Debug)]
pub struct LogSync<L, E, S, Evt> {
    state: State<L>,
    logs: Logs<L>,
    store: S,
    event_tx: broadcast::Sender<Evt>,
    buffer_capacity: usize,
    _marker: PhantomData<E>,
}

impl<L, E, S, Evt> LogSync<L, E, S, Evt> {
    pub fn new(store: S, logs: Logs<L>, event_tx: broadcast::Sender<Evt>) -> Self {
        Self::new_with_capacity(store, logs, event_tx, DEFAULT_BUFFER_CAPACITY)
    }

    pub fn new_with_capacity(
        store: S,
        logs: Logs<L>,
        event_tx: broadcast::Sender<Evt>,
        buffer_capacity: usize,
    ) -> Self {
        Self {
            state: Default::default(),
            store,
            event_tx,
            logs,
            buffer_capacity,
            _marker: PhantomData,
        }
    }
}

impl<L, E, S, Evt> Protocol for LogSync<L, E, S, Evt>
where
    L: LogId + Debug + Send + 'static,
    E: Extensions + Send + 'static,
    S: LogStore<Operation<E>, PublicKey, L, u64, Hash> + Clone + Send + 'static,
    Evt: Debug + From<LogSyncEvent<E>> + Send + 'static,
{
    type Error = LogSyncError;
    type Output = (Dedup<Hash>, LogSyncMetrics);
    type Message = LogSyncMessage<L>;

    async fn run(
        mut self,
        sink: &mut (impl Sink<Self::Message, Error = impl Debug> + Unpin),
        stream: &mut (impl Stream<Item = Result<Self::Message, impl Debug>> + Unpin),
    ) -> Result<Self::Output, Self::Error> {
        let mut sync_done_received = false;
        let mut sync_done_sent = false;
        let mut dedup = Dedup::new(self.buffer_capacity);

        let metrics = loop {
            match self.state {
                State::Start => self.state = State::SendHave,
                State::SendHave => {
                    let local = get_log_heights(&self.store, &self.logs).await?;
                    sink.send(LogSyncMessage::<L>::Have(local.clone()))
                        .await
                        .map_err(|err| LogSyncError::MessageSink(format!("{err:?}")))?;

                    self.state = State::ReceiveHave { local };
                }
                State::ReceiveHave { local } => {
                    let Some(message) = stream.next().await else {
                        return Err(LogSyncError::UnexpectedStreamClosure);
                    };
                    let message =
                        message.map_err(|err| LogSyncError::MessageStream(format!("{err:?}")))?;
                    let LogSyncMessage::Have(remote) = message else {
                        return Err(LogSyncError::UnexpectedMessage(message.to_string()));
                    };

                    let remote_needs = compare(&local, &remote);

                    self.state = State::SendPreSync { remote_needs };
                }
                State::SendPreSync { remote_needs } => {
                    let mut outbound_operations = 0;
                    let mut outbound_bytes = 0;
                    for (public_key, log_range) in remote_needs.iter() {
                        for (log_id, (after, until)) in log_range.iter() {
                            if let Some((inner_outbound_operations, inner_outbound_bytes)) = self
                                .store
                                .get_log_size(public_key, log_id, *after, *until)
                                .await
                                .map_err(|err| LogSyncError::OperationStore(format!("{err}")))?
                            {
                                outbound_operations += inner_outbound_operations;
                                outbound_bytes += inner_outbound_bytes;
                            };
                        }
                    }

                    if outbound_bytes > 0 {
                        sink.send(LogSyncMessage::PreSync {
                            total_operations: outbound_operations,
                            total_bytes: outbound_bytes,
                        })
                        .await
                        .map_err(|err| LogSyncError::MessageSink(format!("{err:?}")))?;
                    } else {
                        sink.send(LogSyncMessage::Done)
                            .await
                            .map_err(|err| LogSyncError::MessageSink(format!("{err:?}")))?;
                        sync_done_sent = true;
                    }

                    self.state = State::ReceivePreSyncOrDone {
                        remote_needs,
                        outbound_operations,
                        outbound_bytes,
                    };
                }
                State::ReceivePreSyncOrDone {
                    remote_needs,
                    outbound_operations,
                    outbound_bytes,
                } => {
                    let Some(message) = stream.next().await else {
                        return Err(LogSyncError::UnexpectedStreamClosure);
                    };
                    let message =
                        message.map_err(|err| LogSyncError::MessageStream(format!("{err:?}")))?;

                    let (inbound_operations, inbound_bytes) = match message {
                        LogSyncMessage::PreSync {
                            total_operations,
                            total_bytes,
                        } => (total_operations, total_bytes),
                        LogSyncMessage::Done => {
                            sync_done_received = true;
                            (0, 0)
                        }
                        message => {
                            return Err(LogSyncError::UnexpectedMessage(message.to_string()));
                        }
                    };

                    debug!(
                        local_ops = outbound_operations,
                        remote_ops = inbound_operations,
                        local_bytes = outbound_bytes,
                        remote_bytes = inbound_bytes,
                        "sync metrics exchanged",
                    );

                    let metrics = LogSyncMetrics {
                        outbound_operations,
                        outbound_bytes,
                        inbound_operations,
                        inbound_bytes,
                        sent_operations: 0,
                        sent_bytes: 0,
                        received_operations: 0,
                        received_bytes: 0,
                    };

                    self.event_tx
                        .send(
                            LogSyncEvent::MetricsExchanged {
                                metrics: metrics.clone(),
                            }
                            .into(),
                        )
                        .map_err(|_| LogSyncError::BroadcastSend)?;

                    self.state = State::Sync {
                        remote_needs,
                        metrics,
                    };
                }
                State::Sync {
                    remote_needs,
                    mut metrics,
                } => {
                    let mut send_logs_len = remote_needs.len();
                    let mut remote_needs = stream::iter(remote_needs);
                    // We perform a loop awaiting futures on both the receiving stream and the list
                    // of log ranges we have to send. This means that processing of both streams is
                    // done concurrently.
                    loop {
                        select! {
                            Some(message) = stream.next(), if !sync_done_received => {
                                let message =
                                    message.map_err(|err| LogSyncError::MessageStream(format!("{err:?}")))?;

                                match message {
                                    LogSyncMessage::Operation(header, body) => {
                                        metrics.received_bytes += {
                                            header.len()
                                                + body.as_ref().map(|bytes| bytes.len()).unwrap_or_default()
                                        } as u64;
                                        metrics.received_operations += 1;

                                        let header: Header<E> = decode_cbor(&header[..])?;
                                        let body = body.map(|ref bytes| Body::new(bytes));

                                        // Insert message hash into deduplication buffer.
                                        dedup.insert(header.hash());

                                        trace!(
                                            phase = "sync",
                                            id = ?header.hash().fmt_short(),
                                            received_ops = metrics.received_operations,
                                            received_bytes = metrics.received_bytes,
                                            "received operation"
                                        );

                                        // Forward data received from the remote to the app layer.
                                        self.event_tx
                                            .send(
                                                LogSyncEvent::OperationReceived{operation:Box::new(Operation{hash:header.hash(),header,body,}), metrics: metrics.clone() }
                                                .into(),
                                            )
                                            .map_err(|_| LogSyncError::BroadcastSend)?;
                                    }
                                    LogSyncMessage::Done => {
                                        trace!(
                                            phase = "sync",
                                            "received done message"
                                        );

                                        sync_done_received = true;
                                    }
                                    message => {
                                        return Err(LogSyncError::UnexpectedMessage(message.to_string()));
                                    }
                                }
                            },
                            Some((author, log_ranges)) = remote_needs.next() => {
                                for (log_id, (after, until)) in log_ranges {
                                    // Get all entries from the log we should send to the remote.
                                    let Some(result) = self
                                    .store
                                    .get_log_entries(&author, &log_id, after, until)
                                    .await
                                    .map_err(|err| LogSyncError::OperationStore(format!("{err}")))? else {
                                        warn!(
                                            author = author.fmt_short(),
                                            log_id = ?log_id,
                                            after = after,
                                            until = until,
                                            "expected log missing from store"
                                        );
                                        continue;
                                    };

                                    for (operation, header_bytes) in result {
                                        let header = operation.header;
                                        let body = operation.body;
                                        let hash = operation.hash;

                                        metrics.sent_bytes += { header_bytes.len() +
                                            body.as_ref().map(|body|
                                        body.to_bytes().len()).unwrap_or_default() } as u64;
                                        metrics.sent_operations += 1;

                                        trace!(
                                            phase = "sync",
                                            public_key = %author.fmt_short(),
                                            log_id = ?log_id,
                                            seq_num = header.seq_num,
                                            id = %hash.fmt_short(),
                                            sent_ops = metrics.sent_operations,
                                            sent_bytes = metrics.sent_bytes,
                                            "send operation",
                                        );

                                        sink.send(LogSyncMessage::Operation(header_bytes, body.as_ref().map(|body|
                                            body.to_bytes())))
                                            .await
                                            .map_err(|err|
                                            LogSyncError::MessageSink(format!("{err:?}")))?;

                                        dedup.insert(hash);
                                    }
                                }
                                // We've finished sending all logs, send sync done message.
                                send_logs_len -= 1;
                                if send_logs_len == 0 {
                                    trace!(
                                        phase = "sync",
                                        public_key = %author.fmt_short(),
                                        "send sync done message",
                                    );
                                    sink.send(LogSyncMessage::Done)
                                        .await
                                        .map_err(|err| LogSyncError::MessageSink(format!("{err:?}")))?;
                                    sync_done_sent = true;
                                }
                            },
                            else => {
                                // If both streams are empty (they return None), or we received a
                                // sync done message and we sent all our pending operations, exit
                                // the loop.
                                if sync_done_received && sync_done_sent {
                                    trace!("sync done sent and received");
                                    break;
                                }
                            }
                        }
                    }
                    self.state = State::End { metrics };
                }
                State::End { metrics } => {
                    break metrics;
                }
            }
        };

        Ok((dedup, metrics))
    }
}

/// Return the local log heights of all passed logs.
async fn get_log_heights<L, E, S>(
    store: &S,
    logs: &Logs<L>,
) -> Result<LogHeights<PublicKey, L>, LogSyncError>
where
    L: LogId,
    S: LogStore<Operation<E>, PublicKey, L, u64, Hash> + Clone + Send + 'static,
{
    let mut result = BTreeMap::new();
    for (public_key, log_ids) in logs {
        let Some(log_heights) = store
            .get_log_heights(public_key, log_ids)
            .await
            .map_err(|err| LogSyncError::LogStore(format!("{err}")))?
        else {
            continue;
        };
        result.insert(*public_key, log_heights);
    }

    Ok(result)
}

/// Protocol messages.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(bound(deserialize = "L: LogId"))]
#[serde(tag = "type", content = "value")]
pub enum LogSyncMessage<L>
where
    L: LogId,
{
    Have(LogHeights<PublicKey, L>),
    PreSync {
        total_operations: u64,
        total_bytes: u64,
    },
    Operation(Vec<u8>, Option<Vec<u8>>),
    Done,
}

impl<L> Display for LogSyncMessage<L>
where
    L: LogId,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let value = match self {
            LogSyncMessage::Have(_) => "have",
            LogSyncMessage::PreSync { .. } => "pre_sync",
            LogSyncMessage::Operation(_, _) => "operation",
            LogSyncMessage::Done => "done",
        };

        write!(f, "{value}")
    }
}

/// Events emitted from log sync sessions.
#[derive(Clone, Debug, PartialEq)]
pub enum LogSyncEvent<E> {
    /// We have calculated and sent the estimated bytes and operations to be sent during this sync
    /// session and received the remotes' estimates.
    ///
    /// This is the first event to occur and will only be sent once.
    MetricsExchanged { metrics: LogSyncMetrics },

    /// An operation has been received from the remote.
    OperationReceived {
        operation: Box<Operation<E>>,
        metrics: LogSyncMetrics,
    },
}

/// Sync metrics emitted in event messages.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct LogSyncMetrics {
    pub outbound_operations: u64,
    pub outbound_bytes: u64,
    pub inbound_operations: u64,
    pub inbound_bytes: u64,
    pub sent_operations: u64,
    pub sent_bytes: u64,
    pub received_operations: u64,
    pub received_bytes: u64,
}

/// Protocol error types.
#[derive(Debug, Error)]
pub enum LogSyncError {
    #[error(transparent)]
    Decode(#[from] DecodeError),

    #[error("log store error: {0}")]
    LogStore(String),

    #[error("operation store error: {0}")]
    OperationStore(String),

    #[error("no active receivers when broadcasting")]
    BroadcastSend,

    #[error("log sync error sending on message sink: {0}")]
    MessageSink(String),

    #[error("log sync error receiving from message stream: {0}")]
    MessageStream(String),

    #[error("remote unexpectedly closed stream during initial sync")]
    UnexpectedStreamClosure,

    #[error("log sync received unexpected protocol message: {0}")]
    UnexpectedMessage(String),
}

/// Returns a displayable string representing the underlying value in a short format, easy to read
/// during debugging and logging.
pub trait ShortFormat {
    fn fmt_short(&self) -> String;
}

impl ShortFormat for PublicKey {
    fn fmt_short(&self) -> String {
        self.to_hex()[0..10].to_string()
    }
}

impl ShortFormat for Hash {
    fn fmt_short(&self) -> String {
        self.to_hex()[0..5].to_string()
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use assert_matches::assert_matches;
    use futures::StreamExt;
    use futures::channel::mpsc;
    use p2panda_core::{Body, Hash};
    use p2panda_store::operations::OperationStore;
    use p2panda_store::{SqliteStore, tx_unwrap};

    use crate::protocols::log_sync::{LogSyncError, LogSyncEvent, Logs, Operation};
    use crate::test_utils::{
        Peer, TestLogSyncMessage, run_protocol, run_protocol_uni, setup_logging,
    };
    use crate::traits::Protocol;

    #[tokio::test]
    async fn log_sync_no_operations() {
        let mut peer: Peer = Peer::new(0).await;

        let (session, mut event_rx) = peer.log_sync_protocol(&Logs::default());
        let ((_, metrics), mut remote_message_rx) = run_protocol_uni(
            session,
            &[
                TestLogSyncMessage::Have(BTreeMap::default()),
                TestLogSyncMessage::Done,
            ],
        )
        .await
        .unwrap();

        let event_metrics = assert_matches!(
            event_rx.recv().await.unwrap(),
            LogSyncEvent::MetricsExchanged { metrics } => metrics
        );

        assert_eq!(metrics.outbound_operations, 0);
        assert_eq!(metrics.outbound_bytes, 0);
        assert_eq!(metrics.inbound_operations, 0);
        assert_eq!(metrics.inbound_bytes, 0);

        assert_eq!(event_metrics, metrics);

        assert_eq!(
            remote_message_rx.recv().await.unwrap(),
            TestLogSyncMessage::Have(BTreeMap::default())
        );

        assert_eq!(
            remote_message_rx.recv().await.unwrap(),
            TestLogSyncMessage::Done
        );
    }

    #[tokio::test]
    async fn log_sync_some_operations() {
        let mut peer = Peer::new(0).await;
        let log_id = 0;

        let body = Body::new("Hello, Sloth!".as_bytes());
        let (header_0, header_bytes_0) = peer.create_operation(&body, log_id).await;
        let (header_1, header_bytes_1) = peer.create_operation(&body, log_id).await;
        let (header_2, header_bytes_2) = peer.create_operation(&body, log_id).await;

        let mut logs = Logs::default();
        logs.insert(peer.id(), vec![log_id]);

        let (session, mut event_rx) = peer.log_sync_protocol(&logs);
        let ((_, final_metrics), remote_message_rx) = run_protocol_uni(
            session,
            &[
                TestLogSyncMessage::Have(BTreeMap::default()),
                TestLogSyncMessage::Done,
            ],
        )
        .await
        .unwrap();

        let expected_bytes = header_0.payload_size
            + header_bytes_0.len() as u64
            + header_1.payload_size
            + header_bytes_1.len() as u64
            + header_2.payload_size
            + header_bytes_2.len() as u64;

        let metrics = assert_matches!(
            event_rx.recv().await.unwrap(),
            LogSyncEvent::MetricsExchanged { metrics } => metrics
        );

        assert_eq!(metrics.outbound_operations, 3);
        assert_eq!(metrics.outbound_bytes, expected_bytes);
        assert_eq!(metrics.inbound_operations, 0);
        assert_eq!(metrics.inbound_bytes, 0);

        assert_eq!(final_metrics.sent_operations, 3);
        assert_eq!(final_metrics.sent_bytes, expected_bytes);
        assert_eq!(final_metrics.received_operations, 0);
        assert_eq!(final_metrics.received_bytes, 0);

        let messages = remote_message_rx.collect::<Vec<_>>().await;

        assert_eq!(messages.len(), 6);

        assert_eq!(
            messages[0],
            TestLogSyncMessage::Have(BTreeMap::from([(peer.id(), BTreeMap::from([(0, 2)]))]))
        );

        assert_eq!(
            messages[1],
            TestLogSyncMessage::PreSync {
                total_operations: 3,
                total_bytes: expected_bytes
            }
        );

        let (header, body_inner) = assert_matches!(
            &messages[2],
            TestLogSyncMessage::Operation(header, Some(body)) => (header.clone(), body.clone())
        );
        assert_eq!(header, header_bytes_0);
        assert_eq!(Body::new(&body_inner), body);

        let (header, body_inner) = assert_matches!(
            &messages[3],
            TestLogSyncMessage::Operation(header, Some(body)) => (header.clone(), body.clone())
        );
        assert_eq!(header, header_bytes_1);
        assert_eq!(Body::new(&body_inner), body);

        let (header, body_inner) = assert_matches!(
            &messages[4],
            TestLogSyncMessage::Operation(header, Some(body)) => (header.clone(), body.clone())
        );
        assert_eq!(header, header_bytes_2);
        assert_eq!(Body::new(&body_inner), body);

        assert_eq!(messages[5], TestLogSyncMessage::Done);
    }

    #[tokio::test]
    async fn log_sync_bidirectional_exchange() {
        setup_logging();

        const LOG_ID: u64 = 0;

        let mut peer_a = Peer::new(0).await;
        let mut peer_b = Peer::new(1).await;

        let body_a = Body::new("From Alice".as_bytes());
        let body_b = Body::new("From Bob".as_bytes());

        let (header_a0, _) = peer_a.create_operation(&body_a, LOG_ID).await;
        let (header_a1, _) = peer_a.create_operation(&body_a, LOG_ID).await;

        let (header_b0, _) = peer_b.create_operation(&body_b, LOG_ID).await;
        let (header_b1, _) = peer_b.create_operation(&body_b, LOG_ID).await;

        let mut logs = Logs::default();
        logs.insert(peer_a.id(), vec![LOG_ID]);
        logs.insert(peer_b.id(), vec![LOG_ID]);

        let (a_session, mut peer_a_event_rx) = peer_a.log_sync_protocol(&logs);
        let (b_session, mut peer_b_event_rx) = peer_b.log_sync_protocol(&logs);

        let ((_, local_metrics), (_, remote_metrics)) =
            run_protocol(a_session, b_session).await.unwrap();

        assert_matches!(
            peer_a_event_rx.recv().await.unwrap(),
            LogSyncEvent::MetricsExchanged { .. }
        );

        let (header, body_inner) = assert_matches!(
            peer_a_event_rx.recv().await.unwrap(),
            LogSyncEvent::OperationReceived { operation, .. } => {
                let Operation { header, body, .. } = *operation;
                (header, body)
            }
        );
        assert_eq!(header, header_b0);
        assert_eq!(body_inner.unwrap(), body_b);

        let (header, body_inner) = assert_matches!(
            peer_a_event_rx.recv().await.unwrap(),
            LogSyncEvent::OperationReceived { operation, .. } => {
                let Operation { header, body, .. } = *operation;
                (header, body)
            }
        );
        assert_eq!(header, header_b1);
        assert_eq!(body_inner.unwrap(), body_b);

        assert_matches!(
            peer_b_event_rx.recv().await.unwrap(),
            LogSyncEvent::MetricsExchanged { .. }
        );

        let (header, body_inner) = assert_matches!(
            peer_b_event_rx.recv().await.unwrap(),
            LogSyncEvent::OperationReceived { operation, .. } => {
                let Operation { header, body, .. } = *operation;
                (header, body)
            }
        );
        assert_eq!(header, header_a0);
        assert_eq!(body_inner.unwrap(), body_a);

        let (header, body_inner) = assert_matches!(
            peer_b_event_rx.recv().await.unwrap(),
            LogSyncEvent::OperationReceived { operation, .. } => {
                let Operation { header, body, .. } = *operation;
                (header, body)
            }
        );
        assert_eq!(header, header_a1);
        assert_eq!(body_inner.unwrap(), body_a);

        assert_eq!(
            local_metrics.inbound_operations,
            local_metrics.received_operations
        );
        assert_eq!(local_metrics.inbound_bytes, local_metrics.received_bytes);
        assert_eq!(
            local_metrics.outbound_operations,
            local_metrics.sent_operations
        );
        assert_eq!(local_metrics.outbound_bytes, local_metrics.sent_bytes);
        assert_eq!(local_metrics.outbound_bytes, remote_metrics.inbound_bytes);
        assert_eq!(
            local_metrics.outbound_operations,
            remote_metrics.inbound_operations
        );
    }

    #[tokio::test]
    async fn log_sync_unexpected_operation_before_presend() {
        let mut peer = Peer::new(0).await;
        const LOG_ID: u64 = 1;

        let body = Body::new(b"unexpected op before presend");
        let (_, header_bytes) = peer.create_operation(&body, LOG_ID).await;

        let mut logs = Logs::default();
        logs.insert(peer.id(), vec![LOG_ID]);

        let (session, _event_rx) = peer.log_sync_protocol(&logs);

        let messages = vec![
            TestLogSyncMessage::Have(BTreeMap::from([(peer.id(), BTreeMap::from([(LOG_ID, 0)]))])),
            TestLogSyncMessage::Operation(header_bytes.clone(), Some(body.to_bytes())),
            TestLogSyncMessage::PreSync {
                total_operations: 1,
                total_bytes: 100,
            },
            TestLogSyncMessage::Done,
        ];

        let result = run_protocol_uni(session, &messages).await;

        assert!(matches!(result, Err(LogSyncError::UnexpectedMessage(_))));
    }

    #[tokio::test]
    async fn log_sync_unexpected_presend_twice() {
        let mut peer = Peer::new(0).await;
        const LOG_ID: u64 = 1;

        let body = Body::new(b"two presends");
        peer.create_operation(&body, LOG_ID).await;

        let mut logs = Logs::default();
        logs.insert(peer.id(), vec![LOG_ID]);

        let (session, _event_rx) = peer.log_sync_protocol(&logs);

        let messages = vec![
            TestLogSyncMessage::Have(BTreeMap::from([(peer.id(), BTreeMap::from([(LOG_ID, 0)]))])),
            TestLogSyncMessage::PreSync {
                total_operations: 1,
                total_bytes: 32,
            },
            TestLogSyncMessage::PreSync {
                total_operations: 1,
                total_bytes: 32,
            },
            TestLogSyncMessage::Done,
        ];

        let result = run_protocol_uni(session, &messages).await;

        assert!(matches!(result, Err(LogSyncError::UnexpectedMessage(_))));
    }

    #[tokio::test]
    async fn log_sync_unexpected_done_before_anything() {
        let mut peer = Peer::new(0).await;
        let logs = Logs::default();

        let (session, _event_rx) = peer.log_sync_protocol(&logs);

        let messages = vec![TestLogSyncMessage::Done];
        let result = run_protocol_uni(session, &messages).await;

        assert!(matches!(result, Err(LogSyncError::UnexpectedMessage(_))));
    }

    #[tokio::test]
    async fn log_sync_unexpected_have_after_presend() {
        let mut peer = Peer::new(0).await;
        const LOG_ID: u64 = 1;

        let body = Body::new(b"bad have order");
        peer.create_operation(&body, LOG_ID).await;

        let mut logs = Logs::default();
        logs.insert(peer.id(), vec![LOG_ID]);

        let (session, _event_rx) = peer.log_sync_protocol(&logs);

        let messages = vec![
            TestLogSyncMessage::Have(BTreeMap::from([(peer.id(), BTreeMap::from([(LOG_ID, 0)]))])),
            TestLogSyncMessage::PreSync {
                total_operations: 1,
                total_bytes: 64,
            },
            TestLogSyncMessage::Have(BTreeMap::from([(
                peer.id(),
                BTreeMap::from([(LOG_ID, 99)]),
            )])),
            TestLogSyncMessage::Done,
        ];

        let result = run_protocol_uni(session, &messages).await;

        assert!(matches!(result, Err(LogSyncError::UnexpectedMessage(_))));
    }

    #[tokio::test]
    async fn log_sync_with_concurrently_pruned_log() {
        setup_logging();

        let mut peer_a = Peer::new(0).await;
        let mut peer_b = Peer::new(1).await;
        let mut peer_c = Peer::new(2).await;

        let body = Body::new(&[0; 1000]);

        for _ in 0..100 {
            let _ = peer_a.create_operation(&body, 0).await;
        }

        for _ in 0..100 {
            let _ = peer_a.create_operation(&body, 1).await;
        }

        let mut to_be_pruned_log = vec![];

        for _ in 0..50 {
            let (header, _) = peer_c.create_operation(&body, 0).await;
            let id = header.hash();

            let operation = Operation {
                hash: header.hash(),
                header,
                body: Some(body.clone()),
            };

            tx_unwrap!(
                &peer_a.store,
                peer_a
                    .store
                    .insert_operation(&id, &operation, &0)
                    .await
                    .unwrap()
            );

            to_be_pruned_log.push(id);
        }

        let mut logs = Logs::default();
        logs.insert(peer_a.id(), vec![0, 1]);
        logs.insert(peer_c.id(), vec![0]);

        let (a_session, _peer_a_event_rx) = peer_a.log_sync_protocol(&logs);
        let (b_session, mut peer_b_event_rx) = peer_b.log_sync_protocol(&logs);

        let _peer_b_event_tx_clone = b_session.event_tx.clone();

        // Spawn a task to run the sync session.
        let session_b_handle = {
            let (mut local_message_tx, local_message_rx) = mpsc::channel(512);
            let (mut remote_message_tx, remote_message_rx) = mpsc::channel(512);
            let mut local_message_rx = local_message_rx.map(Ok::<_, ()>);
            let mut remote_message_rx = remote_message_rx.map(Ok::<_, ()>);
            tokio::spawn(async move {
                a_session
                    .run(&mut local_message_tx, &mut remote_message_rx)
                    .await
                    .unwrap();
            });
            tokio::spawn(async move {
                b_session
                    .run(&mut remote_message_tx, &mut local_message_rx)
                    .await
                    .unwrap()
            })
        };

        loop {
            let event = peer_b_event_rx.recv().await.unwrap();

            if let LogSyncEvent::OperationReceived { .. } = event {
                tx_unwrap!(&peer_a.store, {
                    <SqliteStore as OperationStore<Operation<()>, Hash, ()>>::delete_operation(
                        &peer_a.store,
                        &to_be_pruned_log[0],
                    )
                    .await
                    .unwrap();
                });

                break;
            }
        }

        let (_, metrics) = session_b_handle.await.unwrap();
        assert_eq!(metrics.inbound_operations, 250);
        assert_eq!(metrics.received_operations, 249);
    }
}
