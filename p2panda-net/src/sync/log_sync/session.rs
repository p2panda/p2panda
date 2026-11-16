// SPDX-License-Identifier: MIT OR Apache-2.0

use std::fmt::Debug;
use std::marker::PhantomData;

use futures_channel::mpsc;
use p2panda_core::{AnyOperation, Extensions, Hash, LogId, Operation, SeqNum, Topic, VerifyingKey};
use p2panda_store::logs::LogStore;
use p2panda_store::topics::TopicStore;
use p2panda_sync::ToSync;
use p2panda_sync::protocols::{TopicLogSync, TopicLogSyncEvent};
use tokio::sync::broadcast;

use crate::sync::session::{SessionBuilder, SyncSessionItem};

/// Builder for TopicLogSync sessions.
#[derive(Debug)]
pub(crate) struct LogSyncSessionBuilder<S, L, E> {
    store: S,
    _marker: PhantomData<(L, E)>,
}

impl<S, L, E> LogSyncSessionBuilder<S, L, E> {
    pub(crate) fn new(store: S) -> Self {
        Self {
            store,
            _marker: PhantomData,
        }
    }
}

impl<S: Clone, L, E> Clone for LogSyncSessionBuilder<S, L, E> {
    fn clone(&self) -> Self {
        Self::new(self.store.clone())
    }
}

impl<S, L, E> SessionBuilder for LogSyncSessionBuilder<S, L, E>
where
    S: LogStore<AnyOperation, VerifyingKey, L, SeqNum, Hash>
        + TopicStore<Topic, VerifyingKey, L>
        + Clone
        + Send
        + 'static,
    L: LogId + Debug + Send + 'static,
    E: Extensions + Send + 'static,
{
    type Protocol = TopicLogSync<Topic, S, L, E>;

    /// TODO: Can become AnyOperation.
    type Operation = Operation<E>;
    type Event = TopicLogSyncEvent<E>;

    fn build(
        &self,
        topic: Topic,
        live_rx: Option<mpsc::Receiver<ToSync<Operation<E>>>>,
        events: broadcast::Sender<TopicLogSyncEvent<E>>,
    ) -> Self::Protocol {
        TopicLogSync::new(topic, self.store.clone(), live_rx, events)
    }
}

impl<E> SyncSessionItem<Operation<E>> for TopicLogSyncEvent<E> {
    fn operation(&self) -> Option<&Operation<E>> {
        match self {
            TopicLogSyncEvent::OperationReceived { operation, .. } => Some(operation),
            _ => None,
        }
    }
}
