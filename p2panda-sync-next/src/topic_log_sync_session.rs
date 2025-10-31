// SPDX-License-Identifier: MIT OR Apache-2.0

use std::marker::PhantomData;

use futures::channel::mpsc;
use futures::{AsyncRead, AsyncWrite};
use p2panda_core::Extensions;
use p2panda_store::{LogId, LogStore, OperationStore};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

use crate::cbor::{into_cbor_sink, into_cbor_stream};
use crate::topic_log_sync::{
    LiveModeMessage, Role, TopicLogMap, TopicLogSync, TopicLogSyncError, TopicLogSyncEvent,
    TopicLogSyncMessage,
};
use crate::traits::{Protocol, SyncProtocol, TopicQuery};

pub struct TopicLogSyncSession<T, S, M, L, E> {
    pub store: S,
    pub topic_map: M,
    pub role: Role<T>,
    pub event_tx: mpsc::Sender<TopicLogSyncEvent<T, E>>,
    pub live_mode_rx: Option<broadcast::Receiver<LiveModeMessage<E>>>,
    pub _phantom: PhantomData<(T, L, E)>,
}

impl<T, S, M, L, E> TopicLogSyncSession<T, S, M, L, E>
where
    T: TopicQuery,
    S: LogStore<L, E> + OperationStore<L, E> + Clone,
    M: TopicLogMap<T, L> + Clone,
    L: LogId + for<'de> Deserialize<'de> + Serialize,
    E: Extensions,
{
    pub fn new(
        store: S,
        topic_map: M,
        role: Role<T>,
        live_mode_rx: Option<broadcast::Receiver<LiveModeMessage<E>>>,
        event_tx: mpsc::Sender<TopicLogSyncEvent<T, E>>,
    ) -> Self {
        Self {
            topic_map,
            store,
            role,
            event_tx,
            live_mode_rx,
            _phantom: PhantomData,
        }
    }
}

impl<T, S, M, L, E> SyncProtocol for TopicLogSyncSession<T, S, M, L, E>
where
    T: TopicQuery,
    S: LogStore<L, E> + OperationStore<L, E> + Clone,
    M: TopicLogMap<T, L> + Clone,
    L: LogId + for<'de> Deserialize<'de> + Serialize,
    E: Extensions,
{
    type Error = TopicLogSyncError<T, S, M, L, E>;
    type Event = TopicLogSyncEvent<T, E>;
    type Output = ();

    async fn run(
        self,
        tx: &mut (impl AsyncWrite + Unpin),
        rx: &mut (impl AsyncRead + Unpin),
    ) -> Result<(), TopicLogSyncError<T, S, M, L, E>> {
        // Convert generic read-write channels into framed sink and stream of cbor encoded protocol messages.
        let mut sink = into_cbor_sink::<TopicLogSyncMessage<T, L, E>>(tx);
        let mut stream = into_cbor_stream::<TopicLogSyncMessage<T, L, E>>(rx);

        let protocol = TopicLogSync::new(
            self.store,
            self.topic_map,
            self.role,
            self.live_mode_rx,
            self.event_tx,
        );

        protocol.run(&mut sink, &mut stream).await?;
        Ok(())
    }
}

#[cfg(test)]
pub mod tests {
    use std::collections::HashMap;

    use assert_matches::assert_matches;
    use futures::StreamExt;
    use p2panda_core::{Body, Operation};

    use crate::log_sync::{LogSyncEvent, StatusEvent};
    use crate::test_utils::{Peer, TestTopic, TestTopicSyncEvent, run_topic_sync_session};
    use crate::topic_handshake::TopicHandshakeEvent;
    use crate::topic_log_sync::Role;

    #[tokio::test]
    async fn topic_log_sync_full_duplex() {
        let topic = TestTopic::new("messages");
        let log_id = 0;

        let mut peer_a = Peer::new(0);
        let mut peer_b = Peer::new(1);

        let body = Body::new("Hello, Sloth!".as_bytes());
        let (header_0, _) = peer_a.create_operation(&body, 0).await;
        let (header_1, _) = peer_a.create_operation(&body, 0).await;
        let (header_2, _) = peer_a.create_operation(&body, 0).await;

        let logs = HashMap::from([(peer_a.id(), vec![log_id])]);
        peer_a.insert_topic(&topic, &logs);

        let (peer_a_session, mut peer_a_events_rx, _) =
            peer_a.topic_sync_session(Role::Initiate(topic.clone()), false);

        let (peer_b_session, mut peer_b_events_rx, _) =
            peer_b.topic_sync_session(Role::Accept, false);

        run_topic_sync_session(peer_a_session, peer_b_session)
            .await
            .unwrap();

        let mut index = 0;
        while let Some(event) = peer_a_events_rx.next().await {
            match index {
                0 => assert_matches!(
                    event,
                    TestTopicSyncEvent::Handshake(TopicHandshakeEvent::Initiate(
                        sent_topic,
                    ))
                    if sent_topic == topic
                ),
                1 => assert_eq!(
                    event,
                    TestTopicSyncEvent::Handshake(TopicHandshakeEvent::Done)
                ),
                2 => assert_matches!(
                    event,
                    TestTopicSyncEvent::Sync(LogSyncEvent::Status(StatusEvent::Started { .. }),)
                ),
                3 => {
                    assert_matches!(
                        event,
                        TestTopicSyncEvent::Sync(LogSyncEvent::Status(
                            StatusEvent::Progress { .. }
                        ),)
                    );
                }
                4 => {
                    assert_matches!(
                        event,
                        TestTopicSyncEvent::Sync(LogSyncEvent::Status(
                            StatusEvent::Progress { .. }
                        ),)
                    );
                }
                5 => {
                    assert_matches!(
                        event,
                        TestTopicSyncEvent::Sync(LogSyncEvent::Status(
                            StatusEvent::Completed { .. }
                        ),)
                    );
                    break;
                }
                _ => panic!(),
            };
            index += 1;
        }

        let mut index = 0;
        while let Some(event) = peer_b_events_rx.next().await {
            match index {
                0 => assert_eq!(
                    event,
                    TestTopicSyncEvent::Handshake(TopicHandshakeEvent::Accept)
                ),
                1 => assert_matches!(
                    event,
                    TestTopicSyncEvent::Handshake(TopicHandshakeEvent::TopicReceived(received_topic))
                    if received_topic == topic
                ),
                2 => assert_eq!(
                    event,
                    TestTopicSyncEvent::Handshake(TopicHandshakeEvent::Done)
                ),
                3 => {
                    assert_matches!(
                        event,
                        TestTopicSyncEvent::Sync(LogSyncEvent::Status(StatusEvent::Started { .. }))
                    );
                }
                4 => {
                    assert_matches!(
                        event,
                        TestTopicSyncEvent::Sync(LogSyncEvent::Status(
                            StatusEvent::Progress { .. }
                        ))
                    );
                }
                5 => {
                    assert_matches!(
                        event,
                        TestTopicSyncEvent::Sync(LogSyncEvent::Status(
                            StatusEvent::Progress { .. }
                        ))
                    );
                }
                6 => {
                    let (header, body_inner) = assert_matches!(
                    event,
                    TestTopicSyncEvent::Sync (
                        LogSyncEvent::Data(operation)
                    ) => {let Operation {header, body, ..} = *operation; (header, body)});
                    assert_eq!(header, header_0);
                    assert_eq!(body_inner.unwrap(), body);
                }
                7 => {
                    let (header, body_inner) = assert_matches!(
                    event,
                    TestTopicSyncEvent::Sync (
                        LogSyncEvent::Data(operation)
                    ) => {let Operation {header, body, ..} = *operation; (header, body)});
                    assert_eq!(header, header_1);
                    assert_eq!(body_inner.unwrap(), body);
                }
                8 => {
                    let (header, body_inner) = assert_matches!(
                    event,
                    TestTopicSyncEvent::Sync (
                        LogSyncEvent::Data(operation)
                    ) => {let Operation {header, body, ..} = *operation; (header, body)});
                    assert_eq!(header, header_2);
                    assert_eq!(body_inner.unwrap(), body);
                }
                9 => {
                    assert_matches!(
                        event,
                        TestTopicSyncEvent::Sync(LogSyncEvent::Status(
                            StatusEvent::Completed { .. }
                        ),)
                    );
                    break;
                }
                _ => panic!(),
            };
            index += 1;
        }
    }
}
