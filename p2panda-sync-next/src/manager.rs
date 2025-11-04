use std::collections::HashMap;
use std::fmt::Debug;
use std::future::ready;
use std::marker::PhantomData;

use futures::channel::mpsc;
use futures::stream::{Map, SelectAll};
use futures::{Sink, SinkExt, StreamExt};
use p2panda_core::cbor::decode_cbor;
use p2panda_core::{Body, Extensions, Header};
use p2panda_store::{LogId, LogStore, OperationStore};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::ToSync;
use crate::log_sync::LogSyncEvent;
use crate::topic_log_sync::{
    LiveModeMessage, Role, TopicLogMap, TopicLogSync, TopicLogSyncError, TopicLogSyncEvent,
};
use crate::traits::{SyncManager, TopicQuery};

type SessionEventReceiver<T, E> = Map<
    mpsc::Receiver<TopicLogSyncEvent<T, E>>,
    Box<dyn FnMut(TopicLogSyncEvent<T, E>) -> TopicSyncManagerEvent<T, E>>,
>;

#[derive(Clone, Debug)]
pub struct TopicSyncManagerEvent<T, E> {
    pub session_id: u64,
    pub event: TopicLogSyncEvent<T, E>,
}

pub struct TopicSyncManager<T, S, M, L, E> {
    pub(crate) topic_map: M,
    pub(crate) store: S,
    pub(crate) session_tx_map: HashMap<u64, mpsc::Sender<LiveModeMessage<E>>>,
    pub(crate) events_rx_set: SelectAll<SessionEventReceiver<T, E>>,
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
            session_tx_map: Default::default(),
            events_rx_set: SelectAll::new(),
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
    type Event = TopicSyncManagerEvent<T, E>;
    type Error = LogManagerError<T, S, M, L, E>;

    fn session(&mut self, session_id: u64) -> Self::Protocol {
        let (sub_tx, sub_rx) = mpsc::channel(128);
        self.session_tx_map.insert(session_id, sub_tx.clone());
        let (event_tx, event_rx) = mpsc::channel(128);

        let f: Box<dyn FnMut(TopicLogSyncEvent<T, E>) -> TopicSyncManagerEvent<T, E> + 'static> =
            Box::new(move |event| TopicSyncManagerEvent { session_id, event });

        self.events_rx_set.push(event_rx.map(f));

        TopicLogSync::new(
            self.store.clone(),
            self.topic_map.clone(),
            Role::Accept,
            Some(sub_rx),
            event_tx,
        )
    }

    fn session_handle(
        &self,
        session_id: u64,
    ) -> Option<impl Sink<ToSync, Error = Self::Error> + 'static> {
        self.session_tx_map.get(&session_id).cloned().map(
            |sink: mpsc::Sender<LiveModeMessage<E>>| {
                sink.with(|to_sync| {
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
                                Ok::<_, LogManagerError<T, S, M, L, E>>(LiveModeMessage::FromSub {
                                    header,
                                    body,
                                })
                            }
                            ToSync::Close => {
                                Ok::<_, LogManagerError<T, S, M, L, E>>(LiveModeMessage::Close)
                            }
                        }
                    })
                })
            },
        )
    }

    async fn next_event(&mut self) -> Result<Option<Self::Event>, Self::Error> {
        let event = self.events_rx_set.next().await;
        let Some(event) = event else {
            return Ok(None);
        };

        // If this is a sync or live-mode event containing an operation then get the header and
        // body ready for forwarding to relevant sessions.
        let operation = match &event.event {
            TopicLogSyncEvent::Sync(LogSyncEvent::Data(operation)) => {
                let operation = operation.clone();
                Some((operation.header, operation.body))
            }
            TopicLogSyncEvent::Live { header, body } => Some((*header.clone(), body.clone())),
            _ => None,
        };

        if let Some((header, body)) = operation {
            let keys: Vec<u64> = self.session_tx_map.keys().cloned().collect();
            for id in keys {
                let mut tx = self.session_tx_map.remove(&id).unwrap();
                let result = tx
                    .send(LiveModeMessage::FromSync {
                        header: header.clone(),
                        body: body.clone(),
                    })
                    .await;

                // If there was an error sending the message on the channel it means the receiver
                // has been dropped, which signifies that the session has already closed. In this
                // case we just silently drop the session sender.
                if result.is_ok() {
                    self.session_tx_map.insert(id, tx);
                }
            }
        }

        Ok(Some(event))
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

    use assert_matches::assert_matches;
    use futures::SinkExt;
    use p2panda_core::{Body, cbor::encode_cbor};

    use crate::log_sync::{LogSyncEvent, StatusEvent};
    use crate::manager::TopicSyncManager;
    use crate::test_utils::{Peer, TestTopic, run_protocol};
    use crate::topic_handshake::TopicHandshakeEvent;
    use crate::topic_log_sync::TopicLogSyncEvent;
    use crate::traits::{Configurable, SyncManager};
    use crate::{SyncSessionConfig, ToSync};

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
        let mut peer_a_session = peer_a_manager.session(SESSION_ID);

        // Configure the sync session for Peer A to be initiator.
        let config = SyncSessionConfig {
            topic: Some(topic),
            live_mode: true,
        };
        peer_a_session.configure(&config).unwrap();

        // Instantiate sync session for Peer B.
        let peer_b_session = peer_b_manager.session(SESSION_ID);

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
        assert_eq!(events.len(), 8);
        for (index, event) in events.into_iter().enumerate() {
            assert_eq!(event.session_id, 0);
            match index {
                0 => assert_matches!(
                    event.event,
                    TopicLogSyncEvent::Handshake(TopicHandshakeEvent::Initiate(ref topic))
                        if topic == &TestTopic::new("messages")
                ),
                1 => assert_matches!(
                    event.event,
                    TopicLogSyncEvent::Handshake(TopicHandshakeEvent::Done)
                ),
                2 => assert_matches!(
                    event.event,
                    TopicLogSyncEvent::Sync(LogSyncEvent::Status(StatusEvent::Started { .. }))
                ),
                3 | 4 => assert_matches!(
                    event.event,
                    TopicLogSyncEvent::Sync(LogSyncEvent::Status(StatusEvent::Progress { .. }))
                ),
                5 => assert_matches!(event.event, TopicLogSyncEvent::Sync(LogSyncEvent::Data(_))),
                6 => assert_matches!(
                    event.event,
                    TopicLogSyncEvent::Sync(LogSyncEvent::Status(StatusEvent::Completed { .. }))
                ),
                7 => assert_matches!(event.event, TopicLogSyncEvent::Close { .. }),
                _ => panic!(),
            }
        }

        // Assert Peer B's events.
        let mut events = Vec::new();
        while let Some(event) = peer_b_manager.next_event().await.unwrap() {
            events.push(event);
        }
        assert_eq!(events.len(), 10);
        for (index, event) in events.into_iter().enumerate() {
            assert_eq!(event.session_id, 0);
            match index {
                0 => assert_matches!(
                    event.event,
                    TopicLogSyncEvent::Handshake(TopicHandshakeEvent::Accept)
                ),
                1 => assert_matches!(
                    event.event,
                    TopicLogSyncEvent::Handshake(
                        TopicHandshakeEvent::TopicReceived(ref topic)
                    ) if topic == &TestTopic::new("messages")
                ),
                2 => assert_matches!(
                    event.event,
                    TopicLogSyncEvent::Handshake(TopicHandshakeEvent::Done)
                ),
                3 => assert_matches!(
                    event.event,
                    TopicLogSyncEvent::Sync(LogSyncEvent::Status(StatusEvent::Started { .. }))
                ),
                4 | 5 => assert_matches!(
                    event.event,
                    TopicLogSyncEvent::Sync(LogSyncEvent::Status(StatusEvent::Progress { .. }))
                ),
                6 => {
                    assert_matches!(event.event, TopicLogSyncEvent::Sync(LogSyncEvent::Data(_)))
                }
                7 => assert_matches!(
                    event.event,
                    TopicLogSyncEvent::Sync(LogSyncEvent::Status(StatusEvent::Completed { .. }))
                ),
                8 => {
                    assert_matches!(event.event, TopicLogSyncEvent::Live { .. })
                }
                9 => assert_matches!(event.event, TopicLogSyncEvent::Close { .. }),
                _ => panic!(),
            }
        }
    }
}
