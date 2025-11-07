// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fmt::Debug;
use std::marker::PhantomData;
use std::pin::Pin;

use futures::channel::mpsc;
use futures::future::ready;
use futures::stream::{Map, SelectAll};
use futures::{Sink, SinkExt, StreamExt};
use p2panda_core::cbor::decode_cbor;
use p2panda_core::{Body, Extensions, Header};
use p2panda_store::{LogId, LogStore, OperationStore};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::log_sync::LogSyncEvent;
use crate::session_topic_map::SessionTopicMap;
use crate::topic_handshake::TopicHandshakeEvent;
use crate::topic_log_sync::{
    LiveModeMessage, Role, TopicLogMap, TopicLogSync, TopicLogSyncError, TopicLogSyncEvent,
};
use crate::traits::{Protocol, SyncManager, TopicQuery};
use crate::{SyncManagerEvent, SyncSessionConfig, ToSync};

type SessionEventReceiver<T, M> =
    Map<mpsc::Receiver<M>, Box<dyn FnMut(M) -> SyncManagerEvent<T, M>>>;

pub struct TopicSyncManager<T, S, M, L, E> {
    pub(crate) topic_map: M,
    pub(crate) store: S,
    pub(crate) session_topic_map: SessionTopicMap<T, mpsc::Sender<LiveModeMessage<E>>>,
    pub(crate) events_rx_set: SelectAll<SessionEventReceiver<T, TopicLogSyncEvent<T, E>>>,
    pub(crate) manager_output_queue: Vec<SyncManagerEvent<T, TopicLogSyncEvent<T, E>>>,
    _phantom: PhantomData<(T, L, E)>,
}

impl<T, S, M, L, E> TopicSyncManager<T, S, M, L, E>
where
    T: TopicQuery,
    E: Clone,
{
    pub fn new(topic_map: M, store: S) -> Self {
        Self {
            topic_map,
            store,
            session_topic_map: SessionTopicMap::default(),
            events_rx_set: SelectAll::new(),
            manager_output_queue: Vec::default(),
            _phantom: PhantomData,
        }
    }
}

impl<T, S, M, L, E> SyncManager<T> for TopicSyncManager<T, S, M, L, E>
where
    T: TopicQuery + 'static,
    M: TopicLogMap<T, L> + Clone + Debug + 'static,
    L: LogId + for<'de> Deserialize<'de> + Serialize + 'static,
    E: Extensions + 'static,
    S: LogStore<L, E> + OperationStore<L, E> + Clone + Debug + 'static,
{
    type Protocol = TopicLogSync<T, S, M, L, E>;
    type Error = LogManagerError<T, S, M, L, E>;

    fn session(&mut self, session_id: u64, config: &SyncSessionConfig<T>) -> Self::Protocol {
        let (live_tx, live_rx) = mpsc::channel(128);
        let role = match &config.topic {
            Some(topic) => {
                self.session_topic_map
                    .insert_with_topic(session_id, topic.clone(), live_tx);
                Role::Initiate(topic.clone())
            }
            None => {
                self.session_topic_map.insert_accepting(session_id, live_tx);
                Role::Accept
            }
        };
        let (event_tx, event_rx) = mpsc::channel(128);

        self.events_rx_set.push(
            event_rx.map(Box::new(move |event| SyncManagerEvent::FromSync {
                session_id,
                event: event,
            })),
        );

        let live_rx = if config.live_mode {
            Some(live_rx)
        } else {
            None
        };

        TopicLogSync::new(
            self.store.clone(),
            self.topic_map.clone(),
            role,
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
    ) -> Result<Option<SyncManagerEvent<T, <Self::Protocol as Protocol>::Event>>, Self::Error> {
        // If the manager has output events queued then these are prioritised.
        if let Some(manager_event) = self.manager_output_queue.pop() {
            return Ok(Some(manager_event));
        }

        let event = self.events_rx_set.next().await;
        let Some(manager_event) = event else {
            return Ok(None);
        };

        let SyncManagerEvent::FromSync { session_id, event } = &manager_event else {
            panic!("only sync events are emitted from session channels");
        };
        let session_id = *session_id;

        // If this is a sync or live-mode event containing an operation then get the header and
        // body ready for forwarding to relevant sessions.
        let operation = match event {
            TopicLogSyncEvent::Sync(LogSyncEvent::Data(operation)) => {
                let operation = operation.clone();
                Some((operation.header, operation.body))
            }
            TopicLogSyncEvent::Live { header, body } => Some((*header.clone(), body.clone())),
            TopicLogSyncEvent::Handshake(TopicHandshakeEvent::Done(topic)) => {
                self.session_topic_map.accepted(session_id, topic.clone());
                self.manager_output_queue
                    .push(SyncManagerEvent::TopicAgreed {
                        session_id,
                        topic: topic.clone(),
                    });
                return Ok(Some(manager_event));
            }
            _ => return Ok(Some(manager_event)),
        };

        if let Some((header, body)) = operation {
            let Some(topic) = self.session_topic_map.topic(session_id) else {
                panic!();
                // TODO: Error("received unexpected operation")
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
                    panic!();
                    // Error("missing session channel")
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

#[derive(Debug, Error)]
pub enum LogManagerError<T, S, M, L, E>
where
    T: TopicQuery,
    S: LogStore<L, E> + OperationStore<L, E> + Clone,
    M: TopicLogMap<T, L>,
{
    #[error(transparent)]
    TopicLogSync(#[from] TopicLogSyncError<T, S, M, L, E>),

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

    use crate::log_sync::{LogSyncEvent, StatusEvent};
    use crate::manager::TopicSyncManager;
    use crate::test_utils::{LogIdExtension, Peer, TestTopic, TestTopicSyncEvent, run_protocol};
    use crate::topic_handshake::TopicHandshakeEvent;
    use crate::topic_log_sync::TopicLogSyncEvent;
    use crate::traits::SyncManager;
    use crate::{SyncManagerEvent, SyncSessionConfig, ToSync};

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
            topic: Some(topic),
            live_mode: true,
        };
        let peer_a_session = peer_a_manager.session(SESSION_ID, &config);

        // Instantiate sync session for Peer B.
        let peer_b_session = peer_b_manager.session(SESSION_ID, &SyncSessionConfig::default());

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
        assert_eq!(events.len(), 9);
        for (index, event) in events.into_iter().enumerate() {
            assert_eq!(event.session_id(), 0);
            match index {
                0 => assert_matches!(
                    event,
                    SyncManagerEvent::FromSync{event: TopicLogSyncEvent::Handshake(TopicHandshakeEvent::Initiate(ref topic)), ..}
                        if topic == &TestTopic::new("messages")
                ),
                1 => assert_matches!(
                    event,
                    SyncManagerEvent::FromSync {
                        event: TopicLogSyncEvent::Handshake(TopicHandshakeEvent::Done(_)),
                        ..
                    }
                ),
                2 => assert_matches!(event, SyncManagerEvent::TopicAgreed { .. }),
                3 => assert_matches!(
                    event,
                    SyncManagerEvent::FromSync {
                        event: TopicLogSyncEvent::Sync(LogSyncEvent::Status(
                            StatusEvent::Started { .. }
                        )),
                        ..
                    }
                ),
                4 | 5 => assert_matches!(
                    event,
                    SyncManagerEvent::FromSync {
                        event: TopicLogSyncEvent::Sync(LogSyncEvent::Status(
                            StatusEvent::Progress { .. }
                        )),
                        ..
                    }
                ),
                6 => assert_matches!(
                    event,
                    SyncManagerEvent::FromSync {
                        event: TopicLogSyncEvent::Sync(LogSyncEvent::Data(_)),
                        ..
                    }
                ),
                7 => assert_matches!(
                    event,
                    SyncManagerEvent::FromSync {
                        event: TopicLogSyncEvent::Sync(LogSyncEvent::Status(
                            StatusEvent::Completed { .. }
                        )),
                        ..
                    }
                ),
                8 => assert_matches!(
                    event,
                    SyncManagerEvent::FromSync {
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
        assert_eq!(events.len(), 11);
        for (index, event) in events.into_iter().enumerate() {
            match index {
                0 => assert_matches!(
                    event,
                    SyncManagerEvent::FromSync {
                        session_id: 0,
                        event: TopicLogSyncEvent::Handshake(TopicHandshakeEvent::Accept)
                    }
                ),
                1 => assert_matches!(
                    event,
                    SyncManagerEvent::FromSync {
                        session_id: 0,
                        event: TopicLogSyncEvent::Handshake(
                            TopicHandshakeEvent::TopicReceived(ref topic)
                        )
                    } if topic == &TestTopic::new("messages")
                ),
                2 => assert_matches!(
                    event,
                    SyncManagerEvent::FromSync {
                        session_id: 0,
                        event: TopicLogSyncEvent::Handshake(TopicHandshakeEvent::Done(_))
                    }
                ),
                3 => assert_matches!(event, SyncManagerEvent::TopicAgreed { .. }),
                4 => assert_matches!(
                    event,
                    SyncManagerEvent::FromSync {
                        session_id: 0,
                        event: TopicLogSyncEvent::Sync(LogSyncEvent::Status(
                            StatusEvent::Started { .. }
                        ))
                    }
                ),
                5 | 6 => assert_matches!(
                    event,
                    SyncManagerEvent::FromSync {
                        session_id: 0,
                        event: TopicLogSyncEvent::Sync(LogSyncEvent::Status(
                            StatusEvent::Progress { .. }
                        ))
                    }
                ),
                7 => assert_matches!(
                    event,
                    SyncManagerEvent::FromSync {
                        session_id: 0,
                        event: TopicLogSyncEvent::Sync(LogSyncEvent::Data(_))
                    }
                ),
                8 => assert_matches!(
                    event,
                    SyncManagerEvent::FromSync {
                        session_id: 0,
                        event: TopicLogSyncEvent::Sync(LogSyncEvent::Status(
                            StatusEvent::Completed { .. }
                        ))
                    }
                ),
                9 => assert_matches!(
                    event,
                    SyncManagerEvent::FromSync {
                        session_id: 0,
                        event: TopicLogSyncEvent::Live { .. }
                    }
                ),
                10 => assert_matches!(
                    event,
                    SyncManagerEvent::FromSync {
                        session_id: 0,
                        event: TopicLogSyncEvent::Close { .. }
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
        let config = SyncSessionConfig {
            topic: Some(topic.clone()),
            live_mode: true,
        };
        let session_ab = manager_a.session(SESSION_AB, &config);
        let session_b = manager_b.session(SESSION_BA, &SyncSessionConfig::default());

        // Session A -> C (A initiates)
        let config = SyncSessionConfig {
            topic: Some(topic.clone()),
            live_mode: true,
        };
        let session_ac = manager_a.session(SESSION_AC, &config);
        let session_c = manager_c.session(SESSION_CA, &SyncSessionConfig::default());

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
        let push_operation =
            |operations: &mut Vec<(Header<LogIdExtension>, Option<Body>)>,
             event: SyncManagerEvent<TestTopic, TestTopicSyncEvent>| {
                let SyncManagerEvent::FromSync { event, .. } = event else {
                    return;
                };

                if let TestTopicSyncEvent::Live { header, body } = &event {
                    operations.push((*header.clone(), body.clone()));
                }

                if let TestTopicSyncEvent::Sync(LogSyncEvent::Data(operation)) = &event {
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
