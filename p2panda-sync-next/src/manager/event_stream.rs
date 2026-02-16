// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fmt::Debug;
use std::hash::Hash as StdHash;
use std::pin::Pin;
use std::task::{Context, Poll};

use futures::channel::mpsc;
use futures::stream::SelectAll;
use futures::{SinkExt, Stream, StreamExt};
use p2panda_core::Extensions;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::wrappers::errors::BroadcastStreamRecvError;
use tracing::{debug, trace};

use crate::manager::{SessionStream, SessionTopicMap, ToTopicSync};
use crate::protocols::TopicLogSyncEvent;
use crate::{FromSync, ToSync};

pub(crate) trait StreamDebug<Item>: Stream<Item = Item> + Send + Debug + 'static {}

impl<T, Item> StreamDebug<Item> for T where T: Stream<Item = Item> + Send + Debug + 'static {}

#[allow(clippy::type_complexity)]
pub(crate) struct ManagerEventStreamState<T, E>
where
    T: Clone + Eq + StdHash + Send + 'static,
    E: Extensions + Send + 'static,
{
    pub(crate) manager_rx: mpsc::Receiver<SessionStream<T, E>>,
    pub(crate) session_rx_set:
        SelectAll<Pin<Box<dyn StreamDebug<Option<FromSync<TopicLogSyncEvent<E>>>>>>>,
    pub(crate) session_topic_map: SessionTopicMap<T, mpsc::Sender<ToTopicSync<E>>>,
}

type FutureOutput<T, E> = (
    ManagerEventStreamState<T, E>,
    Option<FromSync<TopicLogSyncEvent<E>>>,
);

/// Event stream for a manager returned from SyncManager::subscribe().
///
/// Calling `next_event` on the manager event stream returns the next event in the event queue
/// (combined events of all running sync sessions). If the event contains an operation then it
/// will be forwarded on to any concurrently running sync sessions.
#[allow(clippy::type_complexity)]
pub struct ManagerEventStream<T, E>
where
    T: Clone + Eq + StdHash + Send + 'static,
    E: Extensions + Send + 'static,
{
    /// Stream state.
    pub(crate) state: Option<ManagerEventStreamState<T, E>>,

    /// The current future being polled.
    pub(crate) pending: Option<Pin<Box<dyn Future<Output = FutureOutput<T, E>> + Send>>>,
}

impl<T, E> ManagerEventStream<T, E>
where
    T: Clone + Debug + Eq + StdHash + Send + 'static,
    E: Extensions + Send + 'static,
{
    async fn next_event(
        mut state: ManagerEventStreamState<T, E>,
    ) -> (
        ManagerEventStreamState<T, E>,
        Option<FromSync<TopicLogSyncEvent<E>>>,
    ) {
        loop {
            tokio::select!(
                biased;
                item = state.manager_rx.next() => {
                    let Some(manager_event) = item else {
                        trace!("manager event stream closed");
                        return (state, None)
                    };
                    trace!("manager event received: {manager_event:?}");
                    let session_id = manager_event.session_id;
                    state.session_topic_map.insert_with_topic(session_id, manager_event.topic, manager_event.live_tx);

                    let stream = BroadcastStream::new(manager_event.event_rx);

                    let stream =
                        Box::pin(stream.map(Box::new(
                            move |event: Result<TopicLogSyncEvent<E>, BroadcastStreamRecvError>| {
                                event.ok().map(|event| FromSync {
                                    session_id,
                                    remote: manager_event.remote,
                                    event,
                                })
                            },
                        )));
                    state.session_rx_set.push(stream);
                }
                Some(Some(from_sync)) = state.session_rx_set.next() => {
                    trace!("from sync event received: {from_sync:?}");
                    let session_id = from_sync.session_id();
                    let event = from_sync.event();

                    let operation = match event {
                        TopicLogSyncEvent::Operation(operation) => Some(*operation.clone()),
                        _ => return (state, Some(from_sync)),
                    };

                    if let Some(operation) = operation {
                        let Some(topic) = state.session_topic_map.topic(session_id) else {
                            debug!(session_id, "drop session: missing from topic map");
                            state.session_topic_map.drop(session_id);
                            continue;
                        };
                        let keys = state.session_topic_map.sessions(topic);
                        let mut dropped = vec![];

                        for id in keys {
                            if id == session_id {
                                continue;
                            }

                            let Some(tx) = state.session_topic_map.sender_mut(id) else {
                                debug!(session_id = id, "drop session: channel unexpectedly closed");
                                state.session_topic_map.drop(session_id);
                                continue;
                            };

                            let result = tx.send(ToSync::Payload(operation.clone())).await;

                            if result.is_err() {
                                dropped.push(id);
                            }
                        }

                        for id in dropped {
                            debug!(session_id = id, "drop session: channel unexpectedly closed");
                            state.session_topic_map.drop(id);
                        }
                    }

                    return (state, Some(from_sync))
                }
            )
        }
    }
}

impl<T, E> Unpin for ManagerEventStream<T, E>
where
    T: Clone + Debug + Eq + StdHash + Send + 'static,
    E: Extensions + Send + 'static,
{
}

impl<T, E> Stream for ManagerEventStream<T, E>
where
    T: Clone + Debug + Eq + StdHash + Send + 'static,
    E: Extensions + Send + 'static,
{
    type Item = FromSync<TopicLogSyncEvent<E>>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.pending.is_none() {
            let fut = Box::pin(ManagerEventStream::next_event(
                self.state.take().expect("state is not None"),
            ));
            self.pending = Some(fut);
        }

        let fut = self.pending.as_mut().unwrap();
        match fut.as_mut().poll(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready((state, item)) => {
                self.pending = None;
                self.state.replace(state);
                Poll::Ready(item)
            }
        }
    }
}
