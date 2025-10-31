// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::HashMap;
use std::fmt::Debug;
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

/// A map of author logs.
pub type Logs<L> = HashMap<PublicKey, Vec<L>>;

/// Sync session life-cycle states.
#[derive(Default)]
pub enum State {
    /// Initialise session metrics and announce sync start on event stream.
    #[default]
    Start,

    /// Calculate local log heights and send Have message to remote.
    SendHave { metrics: Metrics },

    /// Receive have message from remote and calculate operation diff.
    ReceiveHave { metrics: Metrics },

    /// Send PreSync message to remote or Done if we have nothing to send.
    SendPreSyncOrDone {
        operations: Vec<Hash>,
        metrics: Metrics,
    },

    /// Receive PreSync message from remote or Done if they have nothing to send.
    ReceivePreSyncOrDone {
        operations: Vec<Hash>,
        metrics: Metrics,
    },

    /// Enter sync loop where we exchange operations with the remote, moves onto next state when
    /// both peers have send Done messages.
    Sync {
        operations: Vec<Hash>,
        metrics: Metrics,
    },

    /// Announce on the event stream that the sync session successfully completed.
    End { metrics: Metrics },
}

/// Efficient sync protocol for append-only log data types.
pub struct LogSyncProtocol<L, E, S, Evt> {
    state: State,
    logs: Logs<L>,
    store: S,
    event_tx: mpsc::Sender<Evt>,
    _marker: PhantomData<E>,
}

impl<L, E, S, Evt> LogSyncProtocol<L, E, S, Evt> {
    pub fn new(store: S, logs: Logs<L>, event_tx: mpsc::Sender<Evt>) -> Self {
        Self {
            state: Default::default(),
            store,
            event_tx,
            logs,
            _marker: PhantomData,
        }
    }
}

impl<L, E, S, Evt> Protocol for LogSyncProtocol<L, E, S, Evt>
where
    L: LogId + for<'de> Deserialize<'de> + Serialize,
    E: Extensions,
    S: LogStore<L, E> + OperationStore<L, E>,
    Evt: From<LogSyncEvent<E>>,
{
    type Error = LogSyncError<L, E, S>;
    type Output = ();
    type Message = LogSyncMessage<L>;

    async fn run(
        mut self,
        sink: &mut (impl Sink<Self::Message, Error = Self::Error> + Unpin),
        stream: &mut (impl Stream<Item = Result<Self::Message, Self::Error>> + Unpin),
    ) -> Result<(), LogSyncError<L, E, S>> {
        let mut sync_done_received = false;
        let mut sync_done_sent = false;

        loop {
            match self.state {
                State::Start => {
                    let metrics = Metrics::default();
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
                        .await?;
                    self.state = State::ReceiveHave { metrics };
                }
                State::ReceiveHave { mut metrics } => {
                    let Some(message) = stream.next().await else {
                        return Err(LogSyncError::UnexpectedStreamClosure);
                    };
                    let message = message?;
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
                        .await?;
                    } else {
                        sink.send(LogSyncMessage::Done).await?;
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
                    let message = message?;

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
                    metrics,
                } => {
                    let mut operation_stream = stream::iter(operations);
                    let mut sent_operations = 0;
                    let total_operations = metrics
                        .total_operations_local
                        .expect("total operations set");

                    // We perform a loop awaiting futures on both the receiving stream and the
                    // list of operations we have to send. This means that processing of both
                    // streams is done concurrently. Most importantly, clearing the send stream
                    // will not block processing of the receive stream, and thus avoiding that the
                    // receive buffer grows too large.
                    loop {
                        select! {
                            Some(message) = stream.next(), if !sync_done_received => {
                                let message = message?;
                                match message {
                                    LogSyncMessage::Operation(header, body) => {
                                        // TODO: validate that the operations and bytes received matches the total
                                        // bytes the remote sent in their PreSync message.
                                        let header: Header<E> = decode_cbor(&header[..])?;
                                        let body = body.map(|ref bytes| Body::new(bytes));
                                        // Forward data received from the remote to the app layer.
                                        self.event_tx
                                            .send(LogSyncEvent::Data(Operation { hash: header.hash(), header, body }).into())
                                            .await?;
                                    },
                                    LogSyncMessage::Done => {
                                        sync_done_received = true;
                                    },
                                    message => return Err(LogSyncError::UnexpectedMessage(message))
                                }
                            },
                            Some(hash) = operation_stream.next() => {
                                let (header, body) = self.store.get_raw_operation(hash).await.map_err(LogSyncError::OperationStore)?.expect("operation to be in store");
                                sink.send(LogSyncMessage::Operation(header, body)).await?;
                                sent_operations += 1;
                                if sent_operations >= total_operations {
                                    sink.send(LogSyncMessage::Done).await?;
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
                        .send(LogSyncEvent::Status(StatusEvent::Completed { metrics }).into())
                        .await?;
                    self.event_tx.flush().await?;
                    break;
                }
            }
        }

        sink.flush().await?;
        self.event_tx.flush().await?;

        Ok(())
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

                println!("{size:?}");

                if let Some(size) = size {
                    total_size += size;
                }
            };
        }
    }

    Ok((operations, total_size))
}

/// Protocol messages.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(tag = "type", content = "value")]
pub enum LogSyncMessage<L> {
    Have(Vec<(PublicKey, Vec<(L, u64)>)>),
    PreSync {
        total_operations: u64,
        total_bytes: u64,
    },
    Operation(Vec<u8>, Option<Vec<u8>>),
    Done,
}

/// Events emitted from log sync sessions.
#[derive(Clone, Debug, PartialEq)]
pub enum LogSyncEvent<E> {
    Status(StatusEvent),
    Data(Operation<E>),
}

/// Sync session metrics emitted in session event messages.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Metrics {
    pub total_operations_local: Option<u64>,
    pub total_operations_remote: Option<u64>,
    pub total_bytes_local: Option<u64>,
    pub total_bytes_remote: Option<u64>,
}

/// Sync session status variants carried on log sync session events.
#[derive(Clone, Debug, PartialEq)]
pub enum StatusEvent {
    Started {
        metrics: Metrics,
    },
    Progress {
        metrics: Metrics,
    },
    Completed {
        metrics: Metrics,
    },
    Failed {
        error_message: String,
        metrics: Metrics,
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
    use futures::channel::mpsc::{self, channel};
    use futures::{SinkExt, StreamExt};
    use p2panda_core::{Body, Header, PrivateKey, PublicKey};
    use p2panda_store::{LogStore, MemoryStore, OperationStore};
    use rand::Rng;
    use rand::rngs::StdRng;

    use crate::log_sync::{
        LogSyncError, LogSyncEvent, LogSyncMessage, LogSyncProtocol, Logs, Metrics, Operation,
        StatusEvent,
    };
    use crate::test_utils::create_operation;
    use crate::traits::Protocol;

    type TestLogSyncMessage = LogSyncMessage<u64>;
    type TestLogSyncEvent = LogSyncEvent<()>;
    type TestLogSyncProtocol = LogSyncProtocol<u64, (), MemoryStore<u64>, TestLogSyncEvent>;
    type TestLogSyncError = LogSyncError<u64, (), MemoryStore<u64>>;

    pub struct LogSyncPeer {
        pub private_key: PrivateKey,
        pub store: MemoryStore<u64>,
        pub event_tx: mpsc::Sender<TestLogSyncEvent>,
        pub event_rx: mpsc::Receiver<TestLogSyncEvent>,
    }

    impl LogSyncPeer {
        pub fn new(peer_id: u64) -> Self {
            let mut rng = <StdRng as rand::SeedableRng>::seed_from_u64(peer_id);
            let private_key = PrivateKey::from_bytes(&rng.random());
            let store = MemoryStore::default();
            let (event_tx, event_rx) = mpsc::channel(128);

            Self {
                private_key,
                store,
                event_tx,
                event_rx,
            }
        }

        pub fn id(&self) -> PublicKey {
            self.private_key.public_key()
        }

        pub async fn session(
            &mut self,
            logs: &Logs<u64>,
        ) -> Result<TestLogSyncProtocol, TestLogSyncError> {
            Ok(LogSyncProtocol::new(
                self.store.clone(),
                logs.clone(),
                self.event_tx.clone(),
            ))
        }

        pub async fn create_operation(&mut self, body: &Body, log_id: u64) -> (Header, Vec<u8>) {
            let (seq_num, backlink) = self
                .store
                .latest_operation(&self.id(), &log_id)
                .await
                .unwrap()
                .map(|(header, _)| (header.seq_num + 1, Some(header.hash())))
                .unwrap_or((0, None));

            let (header, header_bytes) =
                create_operation(&self.private_key, body, seq_num, seq_num, backlink);

            self.store
                .insert_operation(header.hash(), &header, Some(body), &header_bytes, &log_id)
                .await
                .unwrap();
            (header, header_bytes)
        }

        pub async fn run(
            session_local: TestLogSyncProtocol,
            session_remote: TestLogSyncProtocol,
        ) -> (Result<(), TestLogSyncError>, Result<(), TestLogSyncError>) {
            let (local_message_tx, local_message_rx) = channel(128);
            let (remote_message_tx, remote_message_rx) = channel(128);

            let mut local_sink = local_message_tx
                .sink_map_err(|err| TestLogSyncError::MessageSink(format!("{err:?}")));
            let mut remote_stream = remote_message_rx.map(|message| Ok(message));

            let local_task = tokio::spawn(async move {
                session_local
                    .run(&mut local_sink, &mut remote_stream)
                    .await?;
                Ok(())
            });

            let mut remote_sink = remote_message_tx
                .sink_map_err(|err| TestLogSyncError::MessageSink(format!("{err:?}")));
            let mut local_stream = local_message_rx.map(|message| Ok(message));

            let remote_task = tokio::spawn(async move {
                session_remote
                    .run(&mut remote_sink, &mut local_stream)
                    .await?;
                Ok(())
            });
            tokio::try_join!(local_task, remote_task).unwrap()
        }

        pub async fn run_uni(
            session_local: TestLogSyncProtocol,
            messages: &[TestLogSyncMessage],
        ) -> (
            Result<(), TestLogSyncError>,
            mpsc::Receiver<TestLogSyncMessage>,
        ) {
            let (mut local_message_tx, local_message_rx) = mpsc::channel(128);
            let (remote_message_tx, remote_message_rx) = mpsc::channel(128);

            for message in messages {
                local_message_tx.send(message.clone()).await.unwrap();
            }

            let mut remote_sink = remote_message_tx
                .sink_map_err(|err| TestLogSyncError::MessageSink(format!("{err:?}")));
            let mut local_stream = local_message_rx.map(|message| Ok(message));

            let result = session_local.run(&mut remote_sink, &mut local_stream).await;
            (result, remote_message_rx)
        }
    }

    #[tokio::test]
    async fn log_sync_no_operations() {
        let mut peer_a = LogSyncPeer::new(0);

        let session = peer_a.session(&Logs::default()).await.unwrap();
        let (result, mut remote_message_rx) = LogSyncPeer::run_uni(
            session,
            &[TestLogSyncMessage::Have(vec![]), TestLogSyncMessage::Done],
        )
        .await;
        assert!(result.is_ok());

        let mut index = 0;
        while let Some(event) = peer_a.event_rx.next().await {
            match index {
                0 => {
                    let (total_operations, total_bytes) = assert_matches!(
                        event,
                        LogSyncEvent::Status(StatusEvent::Started { metrics: Metrics { total_operations_remote, total_bytes_remote, .. } })
                         => (total_operations_remote, total_bytes_remote)
                    );
                    assert_eq!(total_operations, None);
                    assert_eq!(total_bytes, None);
                }
                1 => {
                    let (total_operations, total_bytes) = assert_matches!(
                        event,
                        LogSyncEvent::Status(StatusEvent::Progress { metrics: Metrics { total_operations_local, total_bytes_local, .. } })
                         => (total_operations_local, total_bytes_local)
                    );
                    assert_eq!(total_operations, Some(0));
                    assert_eq!(total_bytes, Some(0));
                }
                2 => {
                    let (total_operations, total_bytes) = assert_matches!(
                        event,
                        LogSyncEvent::Status(StatusEvent::Progress { metrics: Metrics { total_operations_remote, total_bytes_remote, .. } })
                         => (total_operations_remote, total_bytes_remote)
                    );
                    assert_eq!(total_operations, Some(0));
                    assert_eq!(total_bytes, Some(0));
                }
                3 => {
                    let (total_operations, total_bytes) = assert_matches!(
                        event,
                        LogSyncEvent::Status(StatusEvent::Completed { metrics: Metrics { total_operations_remote, total_bytes_remote, .. } })
                         => (total_operations_remote, total_bytes_remote)
                    );
                    assert_eq!(total_operations, Some(0));
                    assert_eq!(total_bytes, Some(0));
                    break;
                }
                _ => panic!(),
            };
            index += 1;
        }

        let mut index = 0;
        while let Some(message) = remote_message_rx.next().await {
            match index {
                0 => assert_eq!(message, TestLogSyncMessage::Have(vec![])),
                1 => {
                    assert_eq!(message, TestLogSyncMessage::Done);
                    break;
                }
                _ => panic!(),
            };
            index += 1;
        }
    }

    #[tokio::test]
    async fn log_sync_some_operations() {
        let mut peer_a = LogSyncPeer::new(0);
        let log_id = 0;

        let body = Body::new("Hello, Sloth!".as_bytes());
        let (header_0, header_bytes_0) = peer_a.create_operation(&body, log_id).await;
        let (header_1, header_bytes_1) = peer_a.create_operation(&body, log_id).await;
        let (header_2, header_bytes_2) = peer_a.create_operation(&body, log_id).await;

        let mut logs = Logs::default();
        logs.insert(peer_a.id(), vec![log_id]);

        let session = peer_a.session(&logs).await.unwrap();
        let (result, mut remote_message_rx) = LogSyncPeer::run_uni(
            session,
            &[TestLogSyncMessage::Have(vec![]), TestLogSyncMessage::Done],
        )
        .await;
        assert!(result.is_ok());

        let expected_bytes = header_0.payload_size
            + header_bytes_0.len() as u64
            + header_1.payload_size
            + header_bytes_1.len() as u64
            + header_2.payload_size
            + header_bytes_2.len() as u64;

        let mut index = 0;
        while let Some(event) = peer_a.event_rx.next().await {
            match index {
                0 => {
                    assert_matches!(event, LogSyncEvent::Status(StatusEvent::Started { .. }));
                }
                1 => {
                    let (total_operations, total_bytes) = assert_matches!(
                        event,
                        LogSyncEvent::Status(StatusEvent::Progress {
                            metrics: Metrics { total_operations_local, total_bytes_local, .. }
                        }) => (total_operations_local, total_bytes_local)
                    );
                    assert_eq!(total_operations, Some(3));

                    assert_eq!(total_bytes, Some(expected_bytes));
                }
                2 => {
                    let (total_operations, total_bytes) = assert_matches!(
                        event,
                        LogSyncEvent::Status(StatusEvent::Progress {
                            metrics: Metrics { total_operations_remote, total_bytes_remote, .. }
                        }) => (total_operations_remote, total_bytes_remote)
                    );
                    assert_eq!(total_operations, Some(0));
                    assert_eq!(total_bytes, Some(0));
                }
                3 => {
                    let (total_operations, total_bytes) = assert_matches!(
                        event,
                        LogSyncEvent::Status(StatusEvent::Completed {
                            metrics: Metrics { total_operations_remote, total_bytes_remote, .. }
                        }) => (total_operations_remote, total_bytes_remote)
                    );
                    assert_eq!(total_operations, Some(0));
                    assert_eq!(total_bytes, Some(0));
                    break;
                }
                _ => panic!(),
            };
            index += 1;
        }

        let mut index = 0;
        while let Some(message) = remote_message_rx.next().await {
            match index {
                0 => assert_eq!(
                    message,
                    TestLogSyncMessage::Have(vec![(peer_a.id(), vec![(0, 2)])])
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
                    break;
                }
                _ => panic!(),
            };
            index += 1;
        }
    }

    #[tokio::test]
    async fn log_sync_bidirectional_exchange() {
        const LOG_ID: u64 = 0;

        let mut peer_a = LogSyncPeer::new(0);
        let mut peer_b = LogSyncPeer::new(1);

        let body_a = Body::new("From Alice".as_bytes());
        let body_b = Body::new("From Bob".as_bytes());

        let (header_a0, _) = peer_a.create_operation(&body_a, LOG_ID).await;
        let (header_a1, _) = peer_a.create_operation(&body_a, LOG_ID).await;

        let (header_b0, _) = peer_b.create_operation(&body_b, LOG_ID).await;
        let (header_b1, _) = peer_b.create_operation(&body_b, LOG_ID).await;

        let mut logs = Logs::default();
        logs.insert(peer_a.id(), vec![LOG_ID]);
        logs.insert(peer_b.id(), vec![LOG_ID]);

        let a_session = peer_a.session(&logs).await.unwrap();
        let b_session = peer_b.session(&logs).await.unwrap();

        let (a_result, b_result) = LogSyncPeer::run(a_session, b_session).await;
        assert!(a_result.is_ok());
        assert!(b_result.is_ok());

        let mut index = 0;
        while let Some(event) = peer_a.event_rx.next().await {
            match index {
                0 => assert_matches!(event, LogSyncEvent::Status(StatusEvent::Started { .. })),
                1 => assert_matches!(event, LogSyncEvent::Status(StatusEvent::Progress { .. })),
                2 => assert_matches!(event, LogSyncEvent::Status(StatusEvent::Progress { .. })),
                3 => {
                    let (header, body_inner) = assert_matches!(
                        event,
                        LogSyncEvent::Data(Operation { header, body }) => (header, body)
                    );
                    assert_eq!(header, header_b0);
                    assert_eq!(body_inner.unwrap(), body_b);
                }
                4 => {
                    let (header, body_inner) = assert_matches!(
                        event,
                        LogSyncEvent::Data(Operation { header, body }) => (header, body)
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
            index += 1;
        }

        let mut index = 0;
        while let Some(event) = peer_b.event_rx.next().await {
            match index {
                0 => assert_matches!(event, LogSyncEvent::Status(StatusEvent::Started { .. })),
                1 => assert_matches!(event, LogSyncEvent::Status(StatusEvent::Progress { .. })),
                2 => assert_matches!(event, LogSyncEvent::Status(StatusEvent::Progress { .. })),
                3 => {
                    let (header, body_inner) = assert_matches!(
                        event,
                        LogSyncEvent::Data(Operation { header, body }) => (header, body)
                    );
                    assert_eq!(header, header_a0);
                    assert_eq!(body_inner.unwrap(), body_a);
                }
                4 => {
                    let (header, body_inner) = assert_matches!(
                        event,
                        LogSyncEvent::Data(Operation { header, body }) => (header, body)
                    );
                    assert_eq!(header, header_a1);
                    assert_eq!(body_inner.unwrap(), body_a);
                }
                5 => {
                    assert_matches!(event, LogSyncEvent::Status(StatusEvent::Completed { .. }));
                    break;
                }
                _ => panic!(),
            }
            index += 1;
        }
    }

    #[tokio::test]
    async fn log_sync_unexpected_operation_before_presend() {
        let mut peer = LogSyncPeer::new(0);
        const LOG_ID: u64 = 1;

        let body = Body::new(b"unexpected op before presend");
        let (_, header_bytes) = peer.create_operation(&body, LOG_ID).await;

        let mut logs = Logs::default();
        logs.insert(peer.id(), vec![LOG_ID]);

        let session = peer.session(&logs).await.unwrap();

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

        let (result, _) = LogSyncPeer::run_uni(session, &messages).await;
        assert!(matches!(
            result,
            Err(LogSyncError::UnexpectedMessage(
                TestLogSyncMessage::Operation(_, _)
            ))
        ));
    }

    #[tokio::test]
    async fn log_sync_unexpected_presend_twice() {
        let mut peer = LogSyncPeer::new(0);
        const LOG_ID: u64 = 1;

        let body = Body::new(b"two presends");
        peer.create_operation(&body, LOG_ID).await;

        let mut logs = Logs::default();
        logs.insert(peer.id(), vec![LOG_ID]);
        let session = peer.session(&logs).await.unwrap();

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

        let (result, _) = LogSyncPeer::run_uni(session, &messages).await;
        assert!(matches!(
            result,
            Err(LogSyncError::UnexpectedMessage(
                TestLogSyncMessage::PreSync { .. }
            ))
        ));
    }

    #[tokio::test]
    async fn log_sync_unexpected_done_before_anything() {
        let mut peer = LogSyncPeer::new(0);
        let logs = Logs::default();

        let session = peer.session(&logs).await.unwrap();

        let messages = vec![TestLogSyncMessage::Done];
        let (result, _) = LogSyncPeer::run_uni(session, &messages).await;

        assert!(matches!(
            result,
            Err(LogSyncError::UnexpectedMessage(TestLogSyncMessage::Done))
        ));
    }

    #[tokio::test]
    async fn log_sync_unexpected_have_after_presend() {
        let mut peer = LogSyncPeer::new(0);
        const LOG_ID: u64 = 1;
        let body = Body::new(b"bad have order");
        peer.create_operation(&body, LOG_ID).await;

        let mut logs = Logs::default();
        logs.insert(peer.id(), vec![LOG_ID]);
        let session = peer.session(&logs).await.unwrap();

        let messages = vec![
            TestLogSyncMessage::Have(vec![(peer.id(), vec![(LOG_ID, 0)])]),
            TestLogSyncMessage::PreSync {
                total_operations: 1,
                total_bytes: 64,
            },
            TestLogSyncMessage::Have(vec![(peer.id(), vec![(LOG_ID, 99)])]), // invalid here
            TestLogSyncMessage::Done,
        ];

        let (result, _) = LogSyncPeer::run_uni(session, &messages).await;
        assert!(matches!(
            result,
            Err(LogSyncError::UnexpectedMessage(TestLogSyncMessage::Have(_)))
        ));
    }
}
