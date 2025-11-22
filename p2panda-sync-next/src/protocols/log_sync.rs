// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashMap;
use std::error::Error as StdError;
use std::fmt::{Debug, Display};
use std::marker::PhantomData;

use futures::channel::mpsc;
use futures::{Sink, SinkExt, Stream, StreamExt, stream};
use p2panda_core::cbor::{DecodeError, decode_cbor};
use p2panda_core::{Body, Extensions, Hash, Header, Operation, PublicKey};
use p2panda_store::{LogId, LogStore, OperationStore};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::select;

use crate::traits::Protocol;
use crate::{DEFAULT_BUFFER_CAPACITY, Dedup};

/// A map of author logs.
pub type Logs<L> = HashMap<PublicKey, Vec<L>>;

/// Sync session life-cycle states.
#[derive(Default)]
pub enum State {
    /// Initialise session metrics and announce sync start on event stream.
    #[default]
    Start,

    /// Calculate local log heights and send Have message to remote.
    SendHave { metrics: LogSyncMetrics },

    /// Receive have message from remote and calculate operation diff.
    ReceiveHave { metrics: LogSyncMetrics },

    /// Send PreSync message to remote or Done if we have nothing to send.
    SendPreSyncOrDone {
        operations: Vec<Hash>,
        metrics: LogSyncMetrics,
    },

    /// Receive PreSync message from remote or Done if they have nothing to send.
    ReceivePreSyncOrDone {
        operations: Vec<Hash>,
        metrics: LogSyncMetrics,
    },

    /// Enter sync loop where we exchange operations with the remote, moves onto next state when
    /// both peers have send Done messages.
    Sync {
        operations: Vec<Hash>,
        metrics: LogSyncMetrics,
    },

    /// Announce on the event stream that the sync session successfully completed.
    End { metrics: LogSyncMetrics },
}

/// Efficient sync protocol for append-only log data types.
pub struct LogSyncProtocol<L, E, S, Evt> {
    state: State,
    logs: Logs<L>,
    store: S,
    event_tx: mpsc::Sender<Evt>,
    buffer_capacity: usize,
    _marker: PhantomData<E>,
}

impl<L, E, S, Evt> LogSyncProtocol<L, E, S, Evt> {
    pub fn new(store: S, logs: Logs<L>, event_tx: mpsc::Sender<Evt>) -> Self {
        Self::new_with_capacity(store, logs, event_tx, DEFAULT_BUFFER_CAPACITY)
    }

    pub fn new_with_capacity(
        store: S,
        logs: Logs<L>,
        event_tx: mpsc::Sender<Evt>,
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

impl<L, E, S, Evt> Protocol for LogSyncProtocol<L, E, S, Evt>
where
    L: LogId + for<'de> Deserialize<'de> + Serialize + Send + Sync + 'static,
    E: Extensions + Send + Sync + 'static,
    S: LogStore<L, E> + OperationStore<L, E> + Debug + Send + Sync + 'static,
    <S as LogStore<L, E>>::Error: StdError + Send + Sync + 'static,
    <S as OperationStore<L, E>>::Error: StdError + Send + Sync + 'static,
    Evt: From<LogSyncEvent<E>>,
{
    type Error = LogSyncError<L, E, S>;
    type Output = Dedup<Hash>;
    type Message = LogSyncMessage<L>;

    async fn run(
        mut self,
        sink: &mut (impl Sink<Self::Message, Error = impl Debug> + Unpin),
        stream: &mut (impl Stream<Item = Result<Self::Message, impl Debug>> + Unpin),
    ) -> Result<Self::Output, Self::Error> {
        let mut sync_done_received = false;
        let mut sync_done_sent = false;
        let mut dedup = Dedup::new(self.buffer_capacity);

        loop {
            match self.state {
                State::Start => {
                    let metrics = LogSyncMetrics::default();
                    self.event_tx
                        .send(
                            LogSyncEvent::Status(StatusEvent::Started {
                                metrics: metrics.clone(),
                            })
                            .into(),
                        )
                        .await?;
                    self.state = State::SendHave { metrics };
                }
                State::SendHave { metrics } => {
                    let local_log_heights = local_log_heights(&self.store, &self.logs).await?;
                    sink.send(LogSyncMessage::<L>::Have(local_log_heights.clone()))
                        .await
                        .map_err(|err| LogSyncError::MessageSink(format!("{err:?}")))?;
                    self.state = State::ReceiveHave { metrics };
                }
                State::ReceiveHave { mut metrics } => {
                    let Some(message) = stream.next().await else {
                        return Err(LogSyncError::UnexpectedStreamClosure);
                    };
                    let message =
                        message.map_err(|err| LogSyncError::MessageStream(format!("{err:?}")))?;
                    let LogSyncMessage::Have(remote_log_heights) = message else {
                        return Err(LogSyncError::UnexpectedMessage(message));
                    };

                    let remote_log_heights_map: HashMap<PublicKey, Vec<(L, u64)>> =
                        remote_log_heights.clone().into_iter().collect();

                    // We only fetch the hashes of the operations we should send to the remote in
                    // this step. This avoids keeping all headers and payloads in memory, we can
                    // fetch one at a time as they are needed within the sync phase later.
                    let (operations, total_size) = operations_needed_by_remote(
                        &self.store,
                        &self.logs,
                        remote_log_heights_map,
                    )
                    .await?;

                    metrics.total_operations_local = Some(operations.len() as u64);
                    metrics.total_bytes_local = Some(total_size);

                    self.state = State::SendPreSyncOrDone {
                        operations,
                        metrics,
                    };
                }
                State::SendPreSyncOrDone {
                    operations,
                    metrics,
                } => {
                    let total_operations = metrics.total_operations_local.unwrap();
                    let total_bytes = metrics.total_bytes_local.unwrap();

                    if total_operations > 0 {
                        sink.send(LogSyncMessage::PreSync {
                            total_bytes,
                            total_operations,
                        })
                        .await
                        .map_err(|err| LogSyncError::MessageSink(format!("{err:?}")))?;
                    } else {
                        sink.send(LogSyncMessage::Done)
                            .await
                            .map_err(|err| LogSyncError::MessageSink(format!("{err:?}")))?;
                    }

                    self.event_tx
                        .send(
                            LogSyncEvent::Status(StatusEvent::Progress {
                                metrics: metrics.clone(),
                            })
                            .into(),
                        )
                        .await?;

                    self.state = State::ReceivePreSyncOrDone {
                        operations,
                        metrics,
                    };
                }
                State::ReceivePreSyncOrDone {
                    operations,
                    mut metrics,
                } => {
                    let Some(message) = stream.next().await else {
                        return Err(LogSyncError::UnexpectedStreamClosure);
                    };
                    let message =
                        message.map_err(|err| LogSyncError::MessageStream(format!("{err:?}")))?;

                    metrics.total_bytes_remote = Some(0);
                    metrics.total_operations_remote = Some(0);

                    match message {
                        LogSyncMessage::PreSync {
                            total_operations,
                            total_bytes,
                        } => {
                            metrics.total_bytes_remote = Some(total_bytes);
                            metrics.total_operations_remote = Some(total_operations);
                        }
                        LogSyncMessage::Done => sync_done_received = true,
                        message => return Err(LogSyncError::UnexpectedMessage(message)),
                    }

                    self.event_tx
                        .send(
                            LogSyncEvent::Status(StatusEvent::Progress {
                                metrics: metrics.clone(),
                            })
                            .into(),
                        )
                        .await?;

                    self.state = State::Sync {
                        operations,
                        metrics,
                    };
                }
                State::Sync {
                    operations,
                    mut metrics,
                } => {
                    let mut operation_stream = stream::iter(operations);
                    let mut sent_operations = 0;
                    let total_operations = metrics
                        .total_operations_local
                        .expect("total operations set");

                    // We perform a loop awaiting futures on both the receiving stream and the
                    // list of operations we have to send. This means that processing of both
                    // streams is done concurrently.
                    loop {
                        select! {
                            Some(message) = stream.next(), if !sync_done_received => {
                                let message = message.map_err(|err| LogSyncError::MessageStream(format!("{err:?}")))?;
                                match message {
                                    LogSyncMessage::Operation(header, body) => {
                                        metrics.total_bytes_received += {header.len() + body.as_ref().map(|bytes| bytes.len()).unwrap_or_default()} as u64;
                                        metrics.total_operations_received += 1;

                                        // TODO: validate that the operations and bytes received matches the total
                                        // bytes the remote sent in their PreSync message.
                                        let header: Header<E> = decode_cbor(&header[..])?;
                                        let body = body.map(|ref bytes| Body::new(bytes));

                                        // Insert message hash into deduplication buffer.
                                        //
                                        // NOTE: we don't deduplicate any received messages during
                                        // sync as for this session they have not been seen
                                        // before.
                                        dedup.insert(header.hash());

                                        // Forward data received from the remote to the app layer.
                                        self.event_tx
                                            .send(LogSyncEvent::Data(Box::new(Operation { hash: header.hash(), header, body })).into())
                                            .await?;
                                    },
                                    LogSyncMessage::Done => {
                                        sync_done_received = true;
                                    },
                                    message => return Err(LogSyncError::UnexpectedMessage(message))
                                }
                            },
                            Some(hash) = operation_stream.next() => {
                                // Insert message hash into deduplication buffer.
                                dedup.insert(hash);

                                // Fetch raw message bytes and send to remote.
                                let (header, body) = self.store.get_raw_operation(hash).await.map_err(LogSyncError::OperationStore)?.expect("operation to be in store");
                                metrics.total_bytes_sent += {header.len() + body.as_ref().map(|bytes| bytes.len()).unwrap_or_default()} as u64;
                                metrics.total_operations_sent += 1;
                                sink.send(LogSyncMessage::Operation(header, body)).await.map_err(|err| LogSyncError::MessageSink(format!("{err:?}")))?;
                                sent_operations += 1;
                                if sent_operations >= total_operations {
                                    sink.send(LogSyncMessage::Done).await.map_err(|err| LogSyncError::MessageSink(format!("{err:?}")))?;
                                    sync_done_sent = true;
                                }
                            },
                            else => {
                                // If both streams are empty (they return None) exit the loop.
                                break;
                            }
                        }
                        if sync_done_received && sync_done_sent {
                            break;
                        }
                    }
                    self.state = State::End { metrics };
                }
                State::End { metrics } => {
                    self.event_tx
                        .send(
                            LogSyncEvent::Status(StatusEvent::Completed {
                                metrics: metrics.clone(),
                            })
                            .into(),
                        )
                        .await?;
                    break;
                }
            }
        }

        sink.flush()
            .await
            .map_err(|err| LogSyncError::MessageSink(format!("{err:?}")))?;
        self.event_tx.flush().await?;

        Ok(dedup)
    }
}

/// Return the local log heights of all passed logs.
async fn local_log_heights<L, E, S>(
    store: &S,
    logs: &Logs<L>,
) -> Result<Vec<(PublicKey, Vec<(L, u64)>)>, LogSyncError<L, E, S>>
where
    L: LogId,
    S: LogStore<L, E> + OperationStore<L, E>,
{
    let mut local_log_heights = Vec::new();
    for (public_key, log_ids) in logs {
        let mut log_heights = Vec::new();
        for log_id in log_ids {
            let latest = store
                .latest_operation(public_key, log_id)
                .await
                .map_err(LogSyncError::LogStore)?;

            if let Some((header, _)) = latest {
                log_heights.push((log_id.clone(), header.seq_num));
            };
        }
        local_log_heights.push((*public_key, log_heights));
    }

    Ok(local_log_heights)
}

/// Compare the local log heights with the remote log heights for all given logs and return the
/// hashes of all operations the remote needs, as well as the total bytes.
async fn operations_needed_by_remote<L, E, S>(
    store: &S,
    logs: &Logs<L>,
    remote_log_heights_map: HashMap<PublicKey, Vec<(L, u64)>>,
) -> Result<(Vec<Hash>, u64), LogSyncError<L, E, S>>
where
    L: LogId,
    E: Extensions,
    S: LogStore<L, E> + OperationStore<L, E>,
{
    // Now that the topic query has been translated into a collection of logs we want to
    // compare our own local log heights with what the remote sent for this topic query.
    //
    // If our logs are more advanced for any log we should collect the entries for sending.
    let mut operations = Vec::new();
    let mut total_size = 0;

    for (public_key, log_ids) in logs {
        for log_id in log_ids {
            // For all logs in this topic query scope get the local height.
            let latest_operation = store
                .latest_operation(public_key, log_id)
                .await
                .map_err(LogSyncError::LogStore)?;

            let log_height = match latest_operation {
                Some((header, _)) => header.seq_num,
                // If we don't have this log then continue onto the next without
                // sending any messages.
                None => continue,
            };

            // Calculate from which seq num in the log the remote needs operations.
            let remote_needs_from = match remote_log_heights_map.get(public_key) {
                Some(log_heights) => {
                    match log_heights.iter().find(|(id, _)| *id == *log_id) {
                        // The log is known by the remote, take their log height
                        // and plus one.
                        Some((_, log_height)) => log_height + 1,
                        // The log is not known, they need from seq num 0
                        None => 0,
                    }
                }
                // The author is not known, they need from seq num 0.
                None => 0,
            };

            if remote_needs_from <= log_height {
                let log = store
                    .get_log_hashes(public_key, log_id, Some(remote_needs_from))
                    .await
                    .map_err(LogSyncError::LogStore)?;

                if let Some(log) = log {
                    operations.extend(log);
                }

                let size = store
                    .get_log_size(public_key, log_id, Some(remote_needs_from))
                    .await
                    .map_err(LogSyncError::LogStore)?;

                if let Some(size) = size {
                    total_size += size;
                }
            };
        }
    }

    Ok((operations, total_size))
}

/// Protocol messages.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(tag = "type", content = "value")]
pub enum LogSyncMessage<L> {
    Have(Vec<(PublicKey, Vec<(L, u64)>)>),
    PreSync {
        total_operations: u64,
        total_bytes: u64,
    },
    // @TODO: use Header and Body here.
    Operation(Vec<u8>, Option<Vec<u8>>),
    Done,
}

impl<L> Display for LogSyncMessage<L>
where
    L: Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")?;
        Ok(())
    }
}

/// Events emitted from log sync sessions.
#[derive(Clone, Debug, PartialEq)]
pub enum LogSyncEvent<E> {
    Status(StatusEvent),
    Data(Box<Operation<E>>),
}

/// Sync metrics emitted in event messages.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct LogSyncMetrics {
    pub total_operations_local: Option<u64>,
    pub total_operations_remote: Option<u64>,
    pub total_operations_received: u64,
    pub total_operations_sent: u64,
    pub total_bytes_local: Option<u64>,
    pub total_bytes_remote: Option<u64>,
    pub total_bytes_received: u64,
    pub total_bytes_sent: u64,
}

/// Sync status variants sent on log sync events.
#[derive(Clone, Debug, PartialEq)]
pub enum StatusEvent {
    Started {
        metrics: LogSyncMetrics,
    },
    Progress {
        metrics: LogSyncMetrics,
    },
    Completed {
        metrics: LogSyncMetrics,
    },
    Failed {
        error_message: String,
        metrics: LogSyncMetrics,
    },
}

/// Protocol error types.
#[derive(Debug, Error)]
pub enum LogSyncError<L, E, S>
where
    S: LogStore<L, E> + OperationStore<L, E>,
{
    #[error(transparent)]
    Decode(#[from] DecodeError),

    #[error(transparent)]
    LogStore(<S as LogStore<L, E>>::Error),

    #[error(transparent)]
    OperationStore(<S as OperationStore<L, E>>::Error),

    #[error(transparent)]
    MpscSend(#[from] mpsc::SendError),

    #[error("error sending on message sink: {0}")]
    MessageSink(String),

    #[error("error receiving from message stream: {0}")]
    MessageStream(String),

    #[error("stream ended before protocol completion")]
    UnexpectedStreamClosure,

    #[error("received unexpected protocol message: {0}")]
    UnexpectedMessage(LogSyncMessage<L>),
}

#[cfg(test)]
mod tests {
    use assert_matches::assert_matches;
    use futures::StreamExt;
    use p2panda_core::Body;

    use crate::log_sync::{
        LogSyncError, LogSyncEvent, LogSyncMetrics, Logs, Operation, StatusEvent,
    };
    use crate::test_utils::{Peer, TestLogSyncMessage, run_protocol, run_protocol_uni};

    #[tokio::test]
    async fn log_sync_no_operations() {
        let mut peer: Peer = Peer::new(0);

        let (session, event_rx) = peer.log_sync_protocol(&Logs::default());
        let remote_message_rx = run_protocol_uni(
            session,
            &[TestLogSyncMessage::Have(vec![]), TestLogSyncMessage::Done],
        )
        .await
        .unwrap();

        let events = event_rx.collect::<Vec<_>>().await;
        assert_eq!(events.len(), 4);
        for (index, event) in events.into_iter().enumerate() {
            match index {
                0 => {
                    let (total_operations, total_bytes) = assert_matches!(
                        event,
                        LogSyncEvent::Status(StatusEvent::Started { metrics: LogSyncMetrics { total_operations_remote, total_bytes_remote, .. } })
                         => (total_operations_remote, total_bytes_remote)
                    );
                    assert_eq!(total_operations, None);
                    assert_eq!(total_bytes, None);
                }
                1 => {
                    let (total_operations, total_bytes) = assert_matches!(
                        event,
                        LogSyncEvent::Status(StatusEvent::Progress { metrics: LogSyncMetrics { total_operations_local, total_bytes_local, .. } })
                         => (total_operations_local, total_bytes_local)
                    );
                    assert_eq!(total_operations, Some(0));
                    assert_eq!(total_bytes, Some(0));
                }
                2 => {
                    let (total_operations, total_bytes) = assert_matches!(
                        event,
                        LogSyncEvent::Status(StatusEvent::Progress { metrics: LogSyncMetrics { total_operations_remote, total_bytes_remote, .. } })
                         => (total_operations_remote, total_bytes_remote)
                    );
                    assert_eq!(total_operations, Some(0));
                    assert_eq!(total_bytes, Some(0));
                }
                3 => {
                    let (total_operations, total_bytes) = assert_matches!(
                        event,
                        LogSyncEvent::Status(StatusEvent::Completed { metrics: LogSyncMetrics { total_operations_remote, total_bytes_remote, .. } })
                         => (total_operations_remote, total_bytes_remote)
                    );
                    assert_eq!(total_operations, Some(0));
                    assert_eq!(total_bytes, Some(0));
                }
                _ => panic!(),
            };
        }

        let messages = remote_message_rx.collect::<Vec<_>>().await;
        assert_eq!(messages.len(), 2);
        for (index, message) in messages.into_iter().enumerate() {
            match index {
                0 => assert_eq!(message, TestLogSyncMessage::Have(vec![])),
                1 => {
                    assert_eq!(message, TestLogSyncMessage::Done);
                    break;
                }
                _ => panic!(),
            };
        }
    }

    #[tokio::test]
    async fn log_sync_some_operations() {
        let mut peer = Peer::new(0);
        let log_id = 0;

        let body = Body::new("Hello, Sloth!".as_bytes());
        let (header_0, header_bytes_0) = peer.create_operation(&body, log_id).await;
        let (header_1, header_bytes_1) = peer.create_operation(&body, log_id).await;
        let (header_2, header_bytes_2) = peer.create_operation(&body, log_id).await;

        let mut logs = Logs::default();
        logs.insert(peer.id(), vec![log_id]);

        let (session, event_rx) = peer.log_sync_protocol(&logs);
        let remote_message_rx = run_protocol_uni(
            session,
            &[TestLogSyncMessage::Have(vec![]), TestLogSyncMessage::Done],
        )
        .await
        .unwrap();

        let expected_bytes = header_0.payload_size
            + header_bytes_0.len() as u64
            + header_1.payload_size
            + header_bytes_1.len() as u64
            + header_2.payload_size
            + header_bytes_2.len() as u64;

        let events = event_rx.collect::<Vec<_>>().await;
        assert_eq!(events.len(), 4);
        for (index, event) in events.into_iter().enumerate() {
            match index {
                0 => {
                    assert_matches!(event, LogSyncEvent::Status(StatusEvent::Started { .. }));
                }
                1 => {
                    let (total_operations, total_bytes) = assert_matches!(
                        event,
                        LogSyncEvent::Status(StatusEvent::Progress {
                            metrics: LogSyncMetrics { total_operations_local, total_bytes_local, .. }
                        }) => (total_operations_local, total_bytes_local)
                    );
                    assert_eq!(total_operations, Some(3));

                    assert_eq!(total_bytes, Some(expected_bytes));
                }
                2 => {
                    let (total_operations, total_bytes) = assert_matches!(
                        event,
                        LogSyncEvent::Status(StatusEvent::Progress {
                            metrics: LogSyncMetrics { total_operations_remote, total_bytes_remote, .. }
                        }) => (total_operations_remote, total_bytes_remote)
                    );
                    assert_eq!(total_operations, Some(0));
                    assert_eq!(total_bytes, Some(0));
                }
                3 => {
                    let (total_operations, total_bytes) = assert_matches!(
                        event,
                        LogSyncEvent::Status(StatusEvent::Completed {
                            metrics: LogSyncMetrics { total_operations_remote, total_bytes_remote, .. }
                        }) => (total_operations_remote, total_bytes_remote)
                    );
                    assert_eq!(total_operations, Some(0));
                    assert_eq!(total_bytes, Some(0));
                }
                _ => panic!(),
            };
        }

        let messages = remote_message_rx.collect::<Vec<_>>().await;
        assert_eq!(messages.len(), 6);
        for (index, message) in messages.into_iter().enumerate() {
            match index {
                0 => assert_eq!(
                    message,
                    TestLogSyncMessage::Have(vec![(peer.id(), vec![(0, 2)])])
                ),
                1 => assert_eq!(
                    message,
                    TestLogSyncMessage::PreSync {
                        total_operations: 3,
                        total_bytes: expected_bytes
                    }
                ),
                2 => {
                    let (header, body_inner) = assert_matches!(message, TestLogSyncMessage::Operation(
                        header,
                        Some(body),
                    ) => (header, body));
                    assert_eq!(header, header_bytes_0);
                    assert_eq!(Body::new(&body_inner), body)
                }
                3 => {
                    let (header, body_inner) = assert_matches!(message, TestLogSyncMessage::Operation(
                        header,
                        Some(body),
                    ) => (header, body));
                    assert_eq!(header, header_bytes_1);
                    assert_eq!(Body::new(&body_inner), body)
                }
                4 => {
                    let (header, body_inner) = assert_matches!(message, TestLogSyncMessage::Operation(
                        header,
                        Some(body),
                    ) => (header, body));
                    assert_eq!(header, header_bytes_2);
                    assert_eq!(Body::new(&body_inner), body)
                }
                5 => {
                    assert_eq!(message, TestLogSyncMessage::Done);
                }
                _ => panic!(),
            };
        }
    }

    #[tokio::test]
    async fn log_sync_bidirectional_exchange() {
        const LOG_ID: u64 = 0;

        let mut peer_a = Peer::new(0);
        let mut peer_b = Peer::new(1);

        let body_a = Body::new("From Alice".as_bytes());
        let body_b = Body::new("From Bob".as_bytes());

        let (header_a0, _) = peer_a.create_operation(&body_a, LOG_ID).await;
        let (header_a1, _) = peer_a.create_operation(&body_a, LOG_ID).await;

        let (header_b0, _) = peer_b.create_operation(&body_b, LOG_ID).await;
        let (header_b1, _) = peer_b.create_operation(&body_b, LOG_ID).await;

        let mut logs = Logs::default();
        logs.insert(peer_a.id(), vec![LOG_ID]);
        logs.insert(peer_b.id(), vec![LOG_ID]);

        let (a_session, peer_a_event_rx) = peer_a.log_sync_protocol(&logs);
        let (b_session, peer_b_event_rx) = peer_b.log_sync_protocol(&logs);

        run_protocol(a_session, b_session).await.unwrap();

        let events = peer_a_event_rx.collect::<Vec<_>>().await;
        assert_eq!(events.len(), 6);
        for (index, event) in events.into_iter().enumerate() {
            match index {
                0 => assert_matches!(event, LogSyncEvent::Status(StatusEvent::Started { .. })),
                1 => assert_matches!(event, LogSyncEvent::Status(StatusEvent::Progress { .. })),
                2 => assert_matches!(event, LogSyncEvent::Status(StatusEvent::Progress { .. })),
                3 => {
                    let (header, body_inner) = assert_matches!(
                        event,
                        LogSyncEvent::Data(operation) => {let Operation {header, body, ..} = *operation; (header, body)}
                    );
                    assert_eq!(header, header_b0);
                    assert_eq!(body_inner.unwrap(), body_b);
                }
                4 => {
                    let (header, body_inner) = assert_matches!(
                        event,
                        LogSyncEvent::Data(operation) => {let Operation {header, body, ..} = *operation; (header, body)}
                    );
                    assert_eq!(header, header_b1);
                    assert_eq!(body_inner.unwrap(), body_b);
                }
                5 => {
                    assert_matches!(event, LogSyncEvent::Status(StatusEvent::Completed { .. }));
                    break;
                }
                _ => panic!(),
            }
        }

        let events = peer_b_event_rx.collect::<Vec<_>>().await;
        assert_eq!(events.len(), 6);
        for (index, event) in events.into_iter().enumerate() {
            match index {
                0 => assert_matches!(event, LogSyncEvent::Status(StatusEvent::Started { .. })),
                1 => assert_matches!(event, LogSyncEvent::Status(StatusEvent::Progress { .. })),
                2 => assert_matches!(event, LogSyncEvent::Status(StatusEvent::Progress { .. })),
                3 => {
                    let (header, body_inner) = assert_matches!(
                        event,
                        LogSyncEvent::Data(operation) => {let Operation {header, body, ..} = *operation; (header, body)}
                    );
                    assert_eq!(header, header_a0);
                    assert_eq!(body_inner.unwrap(), body_a);
                }
                4 => {
                    let (header, body_inner) = assert_matches!(
                        event,
                        LogSyncEvent::Data(operation) => {let Operation {header, body, ..} = *operation; (header, body)}
                    );
                    assert_eq!(header, header_a1);
                    assert_eq!(body_inner.unwrap(), body_a);
                }
                5 => {
                    let metrics = assert_matches!(event, LogSyncEvent::Status(StatusEvent::Completed { metrics }) => metrics);
                    let LogSyncMetrics {
                        total_operations_local,
                        total_operations_remote,
                        total_operations_received,
                        total_operations_sent,
                        total_bytes_local,
                        total_bytes_remote,
                        total_bytes_received,
                        total_bytes_sent,
                    } = metrics;

                    assert_eq!(total_operations_remote.unwrap(), total_operations_received);
                    assert_eq!(total_bytes_remote.unwrap(), total_bytes_received);
                    assert_eq!(total_operations_local.unwrap(), total_operations_sent);
                    assert_eq!(total_bytes_local.unwrap(), total_bytes_sent);
                }
                _ => panic!(),
            }
        }
    }

    #[tokio::test]
    async fn log_sync_unexpected_operation_before_presend() {
        let mut peer = Peer::new(0);
        const LOG_ID: u64 = 1;

        let body = Body::new(b"unexpected op before presend");
        let (_, header_bytes) = peer.create_operation(&body, LOG_ID).await;

        let mut logs = Logs::default();
        logs.insert(peer.id(), vec![LOG_ID]);

        let (session, _event_rx) = peer.log_sync_protocol(&logs);

        // Remote sends Operation without PreSync first.
        let messages = vec![
            TestLogSyncMessage::Have(vec![(peer.id(), vec![(LOG_ID, 0)])]),
            TestLogSyncMessage::Operation(header_bytes.clone(), Some(body.to_bytes())),
            TestLogSyncMessage::PreSync {
                total_operations: 1,
                total_bytes: 100,
            },
            TestLogSyncMessage::Done,
        ];

        let result = run_protocol_uni(session, &messages).await;
        assert!(matches!(
            result,
            Err(LogSyncError::UnexpectedMessage(
                TestLogSyncMessage::Operation(_, _)
            ))
        ));
    }

    #[tokio::test]
    async fn log_sync_unexpected_presend_twice() {
        let mut peer = Peer::new(0);
        const LOG_ID: u64 = 1;

        let body = Body::new(b"two presends");
        peer.create_operation(&body, LOG_ID).await;

        let mut logs = Logs::default();
        logs.insert(peer.id(), vec![LOG_ID]);
        let (session, _event_rx) = peer.log_sync_protocol(&logs);

        let messages = vec![
            TestLogSyncMessage::Have(vec![(peer.id(), vec![(LOG_ID, 0)])]),
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
        assert!(matches!(
            result,
            Err(LogSyncError::UnexpectedMessage(
                TestLogSyncMessage::PreSync { .. }
            ))
        ));
    }

    #[tokio::test]
    async fn log_sync_unexpected_done_before_anything() {
        let mut peer = Peer::new(0);
        let logs = Logs::default();

        let (session, _event_rx) = peer.log_sync_protocol(&logs);

        let messages = vec![TestLogSyncMessage::Done];
        let result = run_protocol_uni(session, &messages).await;

        assert!(
            matches!(
                result,
                Err(LogSyncError::UnexpectedMessage(TestLogSyncMessage::Done))
            ),
            "{:?}",
            result
        );
    }

    #[tokio::test]
    async fn log_sync_unexpected_have_after_presend() {
        let mut peer = Peer::new(0);
        const LOG_ID: u64 = 1;
        let body = Body::new(b"bad have order");
        peer.create_operation(&body, LOG_ID).await;

        let mut logs = Logs::default();
        logs.insert(peer.id(), vec![LOG_ID]);
        let (session, _event_rx) = peer.log_sync_protocol(&logs);

        let messages = vec![
            TestLogSyncMessage::Have(vec![(peer.id(), vec![(LOG_ID, 0)])]),
            TestLogSyncMessage::PreSync {
                total_operations: 1,
                total_bytes: 64,
            },
            TestLogSyncMessage::Have(vec![(peer.id(), vec![(LOG_ID, 99)])]), // invalid here
            TestLogSyncMessage::Done,
        ];

        let result = run_protocol_uni(session, &messages).await;
        assert!(matches!(
            result,
            Err(LogSyncError::UnexpectedMessage(TestLogSyncMessage::Have(_)))
        ));
    }
}
