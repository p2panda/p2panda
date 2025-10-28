// SPDX-License-Identifier: MIT OR Apache-2.0

//! Test utilities for mocking connector implementations.
//!
//! This module provides mock implementations that can be used across tests to simulate connector
//! behavior without requiring a real backend connection.
use std::collections::HashMap;
use std::convert::Infallible;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};

use anyhow::{Result, bail};
use futures_core::Stream;
use p2panda_core::Hash;
use tokio::sync::{Mutex, broadcast};
use tokio_stream::wrappers::BroadcastStream;

use crate::connector::{Connector, StreamEvent, Subscription, SubscriptionId};
use crate::{Checkpoint, Subject};

#[derive(Debug, Clone, PartialEq)]
pub enum SubscriptionState {
    Active,
    Unsubscribing,
    Unsubscribed,
}

#[derive(Debug)]
struct SubscriptionHandle {
    tx: broadcast::Sender<StreamEvent>,
    state: SubscriptionState,
    subject: Subject,
}

#[derive(Debug)]
struct MockConnectorState {
    next_subscription_id: SubscriptionId,
    subscriptions: HashMap<SubscriptionId, SubscriptionHandle>,
}

impl MockConnectorState {
    fn new() -> Self {
        Self {
            next_subscription_id: 1,
            subscriptions: HashMap::new(),
        }
    }
}

#[derive(Debug)]
pub struct MockEventStream {
    stream: BroadcastStream<StreamEvent>,
}

impl MockEventStream {
    pub fn new(rx: broadcast::Receiver<StreamEvent>) -> Self {
        Self {
            stream: BroadcastStream::new(rx),
        }
    }
}

impl Stream for MockEventStream {
    type Item = Result<StreamEvent, Infallible>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match Pin::new(&mut self.stream).poll_next(cx) {
            Poll::Ready(Some(Ok(event))) => Poll::Ready(Some(Ok(event))),
            Poll::Ready(Some(Err(_))) => {
                // Any error from broadcast stream (lagged, closed) - end the stream.
                Poll::Ready(None)
            }
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}

impl Unpin for MockEventStream {}

#[derive(Debug)]
pub struct MockSubscription {
    id: SubscriptionId,
    connector_state: Arc<Mutex<MockConnectorState>>,
}

impl MockSubscription {
    fn new(id: SubscriptionId, connector_state: Arc<Mutex<MockConnectorState>>) -> Self {
        Self {
            id,
            connector_state,
        }
    }
}

impl Subscription for MockSubscription {
    type Error = Infallible;

    type EventStream = MockEventStream;

    fn id(&self) -> SubscriptionId {
        self.id
    }

    fn events(&self) -> Self::EventStream {
        // @TODO: Do we need to make the trait signature async?
        let state = self.connector_state.try_lock().unwrap();

        if let Some(handle) = state.subscriptions.get(&self.id) {
            let rx = handle.tx.subscribe();
            let tx = handle.tx.clone();

            let subscription_id = self.id;

            drop(state);

            let _ = tx.send(StreamEvent::Subscribed { subscription_id });

            MockEventStream::new(rx)
        } else {
            panic!("subscription should exist");
        }
    }

    async fn replay(&mut self, _from: Checkpoint) -> Result<(), Self::Error> {
        todo!()
    }

    async fn unsubscribe(self) -> Result<(), Self::Error> {
        let mut state = self.connector_state.lock().await;

        if let Some(handle) = state.subscriptions.get_mut(&self.id) {
            match handle.state {
                SubscriptionState::Active => {
                    handle.state = SubscriptionState::Unsubscribing;
                    let _ = handle.tx.send(StreamEvent::Unsubscribed);
                }
                SubscriptionState::Unsubscribing | SubscriptionState::Unsubscribed => {
                    // Nothing to do anymore ..
                }
            }
        }

        Ok(())
    }
}

#[derive(Debug)]
pub struct MockConnector {
    state: Arc<Mutex<MockConnectorState>>,
}

impl MockConnector {
    pub fn new() -> (Self, MockConnectorHandle) {
        let state = Arc::new(Mutex::new(MockConnectorState::new()));

        let connector = Self {
            state: state.clone(),
        };

        let handle = MockConnectorHandle {
            state: state.clone(),
        };

        (connector, handle)
    }
}

#[derive(Debug)]
pub struct MockConnectorHandle {
    state: Arc<Mutex<MockConnectorState>>,
}

impl MockConnectorHandle {
    /// Send an event to a specific subscription.
    pub async fn send_to_subscription(
        &self,
        subscription_id: SubscriptionId,
        event: StreamEvent,
    ) -> Result<()> {
        let state = self.state.lock().await;

        let Some(handle) = state.subscriptions.get(&subscription_id) else {
            bail!("subscriptions not found")
        };

        if handle.state == SubscriptionState::Active {
            if handle.tx.send(event).is_err() {
                bail!("no receivers listening")
            }
        } else {
            bail!("subscription not active")
        }

        Ok(())
    }

    /// Send an event to all active subscriptions for a subject.
    pub async fn send_to_subject(&self, subject: &Subject, event: StreamEvent) -> Result<()> {
        let state = self.state.lock().await;

        for handle in state.subscriptions.values() {
            if handle.subject == *subject && handle.state == SubscriptionState::Active {
                if handle.tx.send(event.clone()).is_err() {
                    bail!("no receivers listening")
                }
            }
        }

        Ok(())
    }

    /// Send an event to all active subscriptions.
    pub async fn send_to_all_subscriptions(&self, event: StreamEvent) -> Result<()> {
        let state = self.state.lock().await;

        for handle in state.subscriptions.values() {
            if handle.state == SubscriptionState::Active {
                if handle.tx.send(event.clone()).is_err() {
                    bail!("no receivers listening")
                }
            }
        }

        Ok(())
    }

    /// Get the current state of a subscription.
    pub async fn subscription_state(
        &self,
        subscription_id: SubscriptionId,
    ) -> Option<SubscriptionState> {
        let state = self.state.lock().await;
        state
            .subscriptions
            .get(&subscription_id)
            .map(|data| data.state.clone())
    }

    /// Get all active subscription ids.
    pub async fn active_subscription_ids(&self) -> Vec<SubscriptionId> {
        let state = self.state.lock().await;
        state
            .subscriptions
            .iter()
            .filter(|(_, data)| data.state == SubscriptionState::Active)
            .map(|(id, _)| *id)
            .collect()
    }
}

impl Connector for MockConnector {
    type Error = Infallible;

    type Subscription = MockSubscription;

    async fn subscribe(
        &self,
        subject: Subject,
        _from: Checkpoint,
        _live: bool,
    ) -> Result<Self::Subscription, Self::Error> {
        let mut state = self.state.lock().await;

        let subscription_id = state.next_subscription_id;
        state.next_subscription_id += 1;

        let (tx, _) = broadcast::channel(128);

        let handle = SubscriptionHandle {
            tx,
            state: SubscriptionState::Active,
            subject,
        };

        state.subscriptions.insert(subscription_id, handle);

        Ok(MockSubscription::new(subscription_id, self.state.clone()))
    }

    async fn publish(
        &self,
        _subject: Subject,
        _header: Vec<u8>,
        _body: Vec<u8>,
    ) -> Result<Hash, Self::Error> {
        todo!()
    }
}
