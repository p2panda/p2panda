// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fmt::Debug;
use std::hash::Hash as StdHash;
use std::marker::PhantomData;
use std::pin::Pin;
use std::sync::Arc;

use futures::channel::mpsc;
use futures::future::ready;
use futures::stream::{Map, SelectAll};
use futures::{Sink, SinkExt, StreamExt};
use p2panda_core::cbor::decode_cbor;
use p2panda_core::{Body, Extensions, Header};
use p2panda_store::{LogId, LogStore, OperationStore};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::Mutex;

use crate::log_sync::LogSyncEvent;
use crate::session_topic_map::SessionTopicMap;
use crate::topic_log_sync::{
    LiveModeMessage, TopicLogMap, TopicLogSync, TopicLogSyncError, TopicLogSyncEvent,
};
use crate::traits::{NetworkRequirements, Protocol, SyncManager};
use crate::{FromSync, SyncSessionConfig, ToSync};

type SessionEventReceiver<M> =
    Map<mpsc::Receiver<M>, Box<dyn FnMut(M) -> FromSync<M> + Send + 'static>>;

/// Create and manage topic log sync sessions.
///
/// Sync sessions are created via the manager, which instantiates them with access to the shared
/// topic map and operation store as well as channels for receiving sync events and for sending
/// newly arriving operations in live mode.
///
/// The manager method `next_event` must be polled in order to consume events coming from any
/// running sync sessions, as well as to allow the manager to perform important event forwarding
/// between sync sessions.
///
/// A handle can be acquired to a sync session via the session_handle method for sending any live
/// mode operations to a specific session. It's expected that users map sessions (by their id) to
/// any topic subscriptions in order to understand the correct mappings.  
#[derive(Clone, Debug)]
#[allow(clippy::type_complexity)]
pub struct TopicSyncManager<T, S, M, L, E> {
    pub(crate) topic_map: M,
    pub(crate) store: S,
    pub(crate) session_topic_map: SessionTopicMap<T, mpsc::Sender<LiveModeMessage<E>>>,
    pub(crate) events_rx_set: Arc<Mutex<SelectAll<SessionEventReceiver<TopicLogSyncEvent<E>>>>>,
    pub(crate) manager_output_queue: Vec<FromSync<TopicLogSyncEvent<E>>>,
    _phantom: PhantomData<L>,
}

impl<T, S, M, L, E> TopicSyncManager<T, S, M, L, E>
where
    E: Clone,
{
    pub fn new(topic_map: M, store: S) -> Self {
        Self {
            topic_map,
            store,
            session_topic_map: SessionTopicMap::default(),
            events_rx_set: Arc::new(Mutex::new(SelectAll::new())),
            manager_output_queue: Vec::default(),
            _phantom: PhantomData,
        }
    }
}

impl<T, S, M, L, E> SyncManager<T> for TopicSyncManager<T, S, M, L, E>
where
    T: Clone + Debug + Eq + StdHash + Serialize + for<'a> Deserialize<'a> + 'static,
    M: TopicLogMap<T, L> + NetworkRequirements,
    L: LogId + for<'de> Deserialize<'de> + Serialize + 'static,
    E: Extensions + 'static,
    S: LogStore<L, E> + OperationStore<L, E> + NetworkRequirements,
{
    type Protocol = TopicLogSync<T, S, M, L, E>;
    type Config = TopicSyncManagerConfig<S, M>;
    type Error = TopicSyncManagerError<T, S, M, L, E>;

    fn from_config(config: Self::Config) -> Self {
        Self::new(config.topic_map, config.store)
    }

    async fn session(&mut self, session_id: u64, config: &SyncSessionConfig<T>) -> Self::Protocol {
        let (live_tx, live_rx) = mpsc::channel(128);
        self.session_topic_map
            .insert_with_topic(session_id, config.topic.clone(), live_tx);
        let (event_tx, event_rx) = mpsc::channel(128);
        let remote = config.remote.clone();

        {
            let mut events_rx_set = self.events_rx_set.lock().await;
            events_rx_set.push(event_rx.map(Box::new(move |event| FromSync { session_id, remote, event })));
        }

        let live_rx = if config.live_mode {
            Some(live_rx)
        } else {
            None
        };

        TopicLogSync::new(
            config.topic.clone(),
            self.store.clone(),
            self.topic_map.clone(),
            live_rx,
            event_tx,
        )
    }

    fn session_handle(
        &self,
        session_id: u64,
    ) -> Option<Pin<Box<dyn Sink<ToSync, Error = Self::Error>>>> {
        let map_fn = |to_sync: ToSync| {
            ready({
                match to_sync {
                    ToSync::Payload(bytes) => {
                        // @TODO(sam): not sure what will be happening at this interface
                        // yet, this code assumes that bytes are sent from p2panda-net and
                        // we decode them here as the topic sync protocol expects messages
                        // to contain decoded types. We could change that so that we send
                        // bytes to the remote and they decode it. Maybe this is
                        // preferable to avoid an extra decoding/encoding round.
                        let (header, body): (Header<E>, Option<Body>) =
                            decode_cbor(&bytes[..]).unwrap();

                        Ok::<_, Self::Error>(LiveModeMessage::Operation {
                            header: Box::new(header),
                            body,
                        })
                    }
                    ToSync::Close => Ok::<_, Self::Error>(LiveModeMessage::Close),
                }
            })
        };

        self.session_topic_map.sender(session_id).map(|tx| {
            Box::pin(tx.clone().with(map_fn)) as Pin<Box<dyn Sink<ToSync, Error = Self::Error>>>
        })
    }

    async fn next_event(
        &mut self,
    ) -> Result<Option<FromSync<<Self::Protocol as Protocol>::Event>>, Self::Error> {
        // If the manager has output events queued then these are prioritised.
        if let Some(manager_event) = self.manager_output_queue.pop() {
            return Ok(Some(manager_event));
        }

        let manager_event = {
            let mut events_rx_set = self.events_rx_set.lock().await;
            match events_rx_set.next().await {
                Some(event) => event,
                None => return Ok(None),
            }
        };

        let session_id = manager_event.session_id();
        let event = manager_event.event();

        // If this is a sync or live-mode event containing an operation then get the header and
        // body ready for forwarding to relevant sessions.
        let operation = match event {
            TopicLogSyncEvent::Sync(LogSyncEvent::Data(operation)) => {
                let operation = operation.clone();
                Some((operation.header, operation.body))
            }
            TopicLogSyncEvent::Live { header, body } => Some((*header.clone(), body.clone())),
            _ => return Ok(Some(manager_event)),
        };

        if let Some((header, body)) = operation {
            let Some(topic) = self.session_topic_map.topic(session_id) else {
                return Err(TopicSyncManagerError::OperationBeforeTopic);
            };
            let keys = self.session_topic_map.sessions(topic);
            let mut dropped = vec![];
            for id in keys {
                // Don't forward messages back to the session they came from.
                if id == session_id {
                    continue;
                }

                // Forward live operation to all concurrent sessions. If they have indeed seen
                // this operation before they will deduplicate it themselves.
                let Some(tx) = self.session_topic_map.sender_mut(id) else {
                    return Err(TopicSyncManagerError::MissingSessionChannel(id));
                };
                let result = tx
                    .send(LiveModeMessage::Operation {
                        header: Box::new(header.clone()),
                        body: body.clone(),
                    })
                    .await;

                // If there was an error sending the message on the channel it means the receiver
                // has been dropped, which signifies that the session has already closed. In this
                // case we just silently drop the session sender.
                if result.is_err() {
                    dropped.push(id);
                }
            }

            for id in dropped {
                self.session_topic_map.drop(id);
            }
        }

        Ok(Some(manager_event))
    }
}

pub struct ManagerHandleInner {

}

pub struct ManagerHandle {}

#[derive(Clone, Debug)]
pub struct TopicSyncManagerConfig<S, M> {
    pub store: S,
    pub topic_map: M,
}

#[derive(Debug, Error)]
pub enum TopicSyncManagerError<T, S, M, L, E>
where
    T: Clone + Debug + Eq + StdHash + Serialize + for<'a> Deserialize<'a>,
    S: LogStore<L, E> + OperationStore<L, E> + Clone,
    M: TopicLogMap<T, L>,
{
    #[error(transparent)]
    TopicLogSync(#[from] TopicLogSyncError<T, S, M, L, E>),

    #[error("received operation before topic agreed")]
    OperationBeforeTopic,

    #[error("missing tx channel for session: {0}")]
    MissingSessionChannel(u64),

    #[error(transparent)]
    Send(#[from] mpsc::SendError),
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::time::Duration;

    use assert_matches::assert_matches;
    use futures::SinkExt;
    use p2panda_core::Header;
    use p2panda_core::{Body, cbor::encode_cbor};

    use crate::TopicSyncManager;
    use crate::log_sync::{LogSyncEvent, StatusEvent};
    use crate::managers::topic_sync_manager::TopicSyncManagerConfig;
    use crate::test_utils::{
        LogIdExtension, Peer, TestMemoryStore, TestTopic, TestTopicMap, TestTopicSyncEvent,
        TestTopicSyncManager, run_protocol,
    };
    use crate::topic_log_sync::TopicLogSyncEvent;
    use crate::traits::SyncManager;
    use crate::{FromSync, SyncSessionConfig, ToSync};

    #[test]
    fn from_config() {
        let store = TestMemoryStore::new();
        let topic_map = TestTopicMap::new();
        let config = TopicSyncManagerConfig { store, topic_map };
        let _: TestTopicSyncManager = SyncManager::from_config(config);
    }

    #[tokio::test]
    async fn manager_e2e() {
        const TOPIC_NAME: &str = "messages";
        const LOG_ID: u64 = 0;
        const SESSION_ID: u64 = 0;

        let topic = TestTopic::new(TOPIC_NAME);

        // Setup Peer A
        let mut peer_a = Peer::new(0);
        let body = Body::new("Hello from Peer A".as_bytes());
        let _ = peer_a.create_operation(&body, LOG_ID).await;
        let logs = HashMap::from([(peer_a.id(), vec![LOG_ID])]);
        peer_a.insert_topic(&topic, &logs);
        let mut peer_a_manager =
            TopicSyncManager::new(peer_a.topic_map.clone(), peer_a.store.clone());

        // Setup Peer B
        let mut peer_b = Peer::new(1);
        let body = Body::new("Hello from Peer B".as_bytes());
        let _ = peer_b.create_operation(&body, LOG_ID).await;
        let logs = HashMap::from([(peer_b.id(), vec![LOG_ID])]);
        peer_b.insert_topic(&topic, &logs);
        let mut peer_b_manager =
            TopicSyncManager::new(peer_b.topic_map.clone(), peer_b.store.clone());

        // Instantiate sync session for Peer A.
        let config = SyncSessionConfig {
            topic,
            remote: peer_b.id(),
            live_mode: true,
        };
        let peer_a_session = peer_a_manager.session(SESSION_ID, &config).await;

        // Instantiate sync session for Peer B.
        let peer_b_session = peer_b_manager.session(SESSION_ID, &config).await;

        // Get a handle to Peer A sync session.
        let mut peer_a_handle = peer_a_manager.session_handle(SESSION_ID).unwrap();

        // Create and send a new live-mode message.
        let (header_1, _) = peer_a.create_operation_no_insert(&body, LOG_ID).await;
        let bytes = encode_cbor(&(header_1.clone(), Some(body.clone()))).unwrap();
        peer_a_handle.send(ToSync::Payload(bytes)).await.unwrap();
        peer_a_handle.send(ToSync::Close).await.unwrap();

        // Actually run the protocol.
        run_protocol(peer_a_session, peer_b_session).await.unwrap();

        // Assert Peer A's events.
        let mut events = Vec::new();
        while let Some(event) = peer_a_manager.next_event().await.unwrap() {
            events.push(event);
        }
        assert_eq!(events.len(), 6);
        for (index, event) in events.into_iter().enumerate() {
            assert_eq!(event.session_id(), 0);
            match index {
                0 => assert_matches!(
                    event,
                    FromSync {
                        event: TopicLogSyncEvent::Sync(LogSyncEvent::Status(
                            StatusEvent::Started { .. }
                        )),
                        ..
                    }
                ),
                1 | 2 => assert_matches!(
                    event,
                    FromSync {
                        event: TopicLogSyncEvent::Sync(LogSyncEvent::Status(
                            StatusEvent::Progress { .. }
                        )),
                        ..
                    }
                ),
                3 => assert_matches!(
                    event,
                    FromSync {
                        event: TopicLogSyncEvent::Sync(LogSyncEvent::Data(_)),
                        ..
                    }
                ),
                4 => assert_matches!(
                    event,
                    FromSync {
                        event: TopicLogSyncEvent::Sync(LogSyncEvent::Status(
                            StatusEvent::Completed { .. }
                        )),
                        ..
                    }
                ),
                5 => assert_matches!(
                    event,
                    FromSync {
                        event: TopicLogSyncEvent::Close { .. },
                        ..
                    }
                ),
                _ => panic!(),
            }
        }

        // Assert Peer B's events.
        let mut events = Vec::new();
        while let Some(event) = peer_b_manager.next_event().await.unwrap() {
            events.push(event);
        }
        assert_eq!(events.len(), 7);
        for (index, event) in events.into_iter().enumerate() {
            match index {
                0 => assert_matches!(
                    event,
                    FromSync {
                        session_id: 0,
                        event: TopicLogSyncEvent::Sync(LogSyncEvent::Status(
                            StatusEvent::Started { .. }
                        )),
                        ..
                    }
                ),
                1 | 2 => assert_matches!(
                    event,
                    FromSync {
                        session_id: 0,
                        event: TopicLogSyncEvent::Sync(LogSyncEvent::Status(
                            StatusEvent::Progress { .. }
                        )),
                        ..
                    }
                ),
                3 => assert_matches!(
                    event,
                    FromSync {
                        session_id: 0,
                        event: TopicLogSyncEvent::Sync(LogSyncEvent::Data(_)),
                        ..
                    }
                ),
                4 => assert_matches!(
                    event,
                    FromSync {
                        session_id: 0,
                        event: TopicLogSyncEvent::Sync(LogSyncEvent::Status(
                            StatusEvent::Completed { .. }
                        )),
                        ..
                    }
                ),
                5 => assert_matches!(
                    event,
                    FromSync {
                        session_id: 0,
                        event: TopicLogSyncEvent::Live { .. },
                        ..
                    }
                ),
                6 => assert_matches!(
                    event,
                    FromSync {
                        session_id: 0,
                        event: TopicLogSyncEvent::Close { .. },
                        ..
                    }
                ),
                _ => panic!(),
            }
        }
    }

    #[tokio::test]
    async fn live_mode_three_peer_forwarding() {
        use std::collections::HashMap;

        const TOPIC_NAME: &str = "chat";
        const LOG_ID: u64 = 0;
        const SESSION_AB: u64 = 0;
        const SESSION_AC: u64 = 1;
        const SESSION_BA: u64 = 2;
        const SESSION_CA: u64 = 3;

        // Shared topic
        let topic = TestTopic::new(TOPIC_NAME);

        // Peer A
        let mut peer_a = Peer::new(0);
        let body_a = Body::new("Hello from A".as_bytes());
        let (peer_a_header_0, _) = peer_a.create_operation(&body_a, LOG_ID).await;
        let logs = HashMap::from([(peer_a.id(), vec![LOG_ID])]);
        peer_a.insert_topic(&topic, &logs);
        let mut manager_a = TopicSyncManager::new(peer_a.topic_map.clone(), peer_a.store.clone());

        // Peer B
        let mut peer_b = Peer::new(1);
        let body_b = Body::new("Hello from B".as_bytes());
        let (peer_b_header_0, _) = peer_b.create_operation(&body_b, LOG_ID).await;
        let logs = HashMap::from([(peer_b.id(), vec![LOG_ID])]);
        peer_b.insert_topic(&topic, &logs);
        let mut manager_b = TopicSyncManager::new(peer_b.topic_map.clone(), peer_b.store.clone());

        // Peer C
        let mut peer_c = Peer::new(2);
        let body_c = Body::new("Hello from C".as_bytes());
        let (peer_c_header_0, _) = peer_c.create_operation(&body_c, LOG_ID).await;
        let logs = HashMap::from([(peer_c.id(), vec![LOG_ID])]);
        peer_c.insert_topic(&topic, &logs);
        let mut manager_c = TopicSyncManager::new(peer_c.topic_map.clone(), peer_c.store.clone());

        // Session A -> B (A initiates)
        let mut config = SyncSessionConfig {
            topic: topic.clone(),
            remote: peer_b.id(),
            live_mode: true,
        };
        let session_ab = manager_a.session(SESSION_AB, &config).await;
        config.remote = peer_a.id();
        let session_b = manager_b.session(SESSION_BA, &config).await;

        // Session A -> C (A initiates)
        let mut config = SyncSessionConfig {
            topic: topic.clone(),
            remote: peer_c.id(),
            live_mode: true,
        };
        let session_ac = manager_a.session(SESSION_AC, &config).await;
        config.remote = peer_a.id();
        let session_c = manager_c.session(SESSION_CA, &config).await;

        // Run both protocols concurrently
        tokio::spawn(async move {
            run_protocol(session_ab, session_b).await.unwrap();
        });
        tokio::spawn(async move {
            run_protocol(session_ac, session_c).await.unwrap();
        });

        // Send live-mode messages from all peers
        let mut handle_ab = manager_a.session_handle(SESSION_AB).unwrap();
        let mut handle_ac = manager_a.session_handle(SESSION_AC).unwrap();
        let mut handle_ba = manager_b.session_handle(SESSION_BA).unwrap();
        let mut handle_ca = manager_c.session_handle(SESSION_CA).unwrap();

        let body_a = Body::new("Hello again from A".as_bytes());
        let body_b = Body::new("Hello again from B".as_bytes());
        let body_c = Body::new("Hello again from C".as_bytes());
        let (peer_a_header_1, _) = peer_a.create_operation(&body_a, LOG_ID).await;
        let (peer_b_header_1, _) = peer_b.create_operation(&body_b, LOG_ID).await;
        let (peer_c_header_1, _) = peer_c.create_operation(&body_c, LOG_ID).await;

        let operation_a = encode_cbor(&(peer_a_header_1.clone(), Some(body_a))).unwrap();
        let operation_b = encode_cbor(&(peer_b_header_1.clone(), Some(body_b))).unwrap();
        let operation_c = encode_cbor(&(peer_c_header_1.clone(), Some(body_c))).unwrap();

        handle_ab
            .send(ToSync::Payload(operation_a.clone()))
            .await
            .unwrap();
        handle_ac.send(ToSync::Payload(operation_a)).await.unwrap();
        handle_ba.send(ToSync::Payload(operation_b)).await.unwrap();
        handle_ca.send(ToSync::Payload(operation_c)).await.unwrap();

        // Collect all operations each peer receives on the event stream.
        let mut operations_a = vec![];
        let mut operations_b = vec![];
        let mut operations_c = vec![];
        let push_operation = |operations: &mut Vec<(Header<LogIdExtension>, Option<Body>)>,
                              event: FromSync<TestTopicSyncEvent>| {

            if let TestTopicSyncEvent::Live { header, body } = event.event() {
                operations.push((*header.clone(), body.clone()));
            }

            if let TestTopicSyncEvent::Sync(LogSyncEvent::Data(operation)) = event.event() {
                operations.push((operation.header.clone(), operation.body.clone()));
            }
        };

        let _ = tokio::time::timeout(Duration::from_millis(500), async {
            loop {
                tokio::select! {
                    Ok(Some(event)) = manager_a.next_event() => {
                        push_operation(&mut operations_a, event)
                    }
                    Ok(Some(event)) = manager_b.next_event() => {
                        push_operation(&mut operations_b, event)

                    }
                    Ok(Some(event)) = manager_c.next_event() => {
                        push_operation(&mut operations_c, event)
                    }
                    else => tokio::time::sleep(Duration::from_millis(5)).await
                }
            }
        })
        .await;

        // All peers received 4 messages, B & C received each other messages via A, and nobody
        // received their own messages.
        assert_eq!(operations_a.len(), 4);
        assert_eq!(operations_b.len(), 4);
        assert_eq!(operations_c.len(), 4);
        assert!(
            operations_a
                .iter()
                .find(|(header, _)| header == &peer_a_header_0 || header == &peer_a_header_1)
                .is_none()
        );
        assert!(
            operations_b
                .iter()
                .find(|(header, _)| header == &peer_b_header_0 || header == &peer_b_header_1)
                .is_none()
        );
        assert!(
            operations_c
                .iter()
                .find(|(header, _)| header == &peer_c_header_0 || header == &peer_c_header_1)
                .is_none()
        );
    }
}
