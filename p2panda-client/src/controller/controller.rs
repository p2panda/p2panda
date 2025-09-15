// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use p2panda_core::Hash;
use thiserror::Error;
use tokio::sync::RwLock;

use crate::controller::backend::{Backend, Subscription, SubscriptionId};
use crate::controller::consumer::Consumer;
use crate::{Checkpoint, Subject};

pub struct Controller<B>
where
    B: Backend,
{
    inner: Arc<Inner<B>>,
}

struct Inner<B>
where
    B: Backend,
{
    backend: Arc<B>,
    checkpoints: RwLock<HashSet<Hash>>,
    subscriptions: RwLock<HashMap<SubscriptionId, B::Subscription>>,
}

impl<B> Clone for Controller<B>
where
    B: Backend,
{
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<B> Controller<B>
where
    B: Backend,
{
    pub fn new(backend: B) -> Self {
        let inner = Inner {
            backend: Arc::new(backend),
            checkpoints: RwLock::new(HashSet::new()),
            subscriptions: RwLock::new(HashMap::new()),
        };

        Self {
            inner: Arc::new(inner),
        }
    }

    pub async fn subscribe(
        &self,
        subject: Subject,
        live: bool,
    ) -> Result<Consumer<B>, ControllerError<B>> {
        let checkpoint = self.get_or_create_checkpoint(&subject).await;

        let subscription = self
            .inner
            .backend
            .subscribe(subject.clone(), checkpoint, live)
            .await
            .map_err(ControllerError::Backend)?;

        let subscription_id = subscription.id();
        let event_stream = subscription.events();

        {
            let mut subscriptions = self.inner.subscriptions.write().await;
            subscriptions.insert(subscription_id.clone(), subscription);
        }

        Ok(Consumer::new(
            subject,
            subscription_id,
            event_stream,
            self.clone(),
        ))
    }

    pub async fn publish(
        &self,
        subject: Subject,
        header: Vec<u8>,
        body: Vec<u8>,
    ) -> Result<Hash, ControllerError<B>> {
        self.inner
            .backend
            .publish(subject, header, body)
            .await
            .map_err(ControllerError::Backend)
    }

    pub async fn replay_from(
        &self,
        subscription_id: SubscriptionId,
        checkpoint: Checkpoint,
    ) -> Result<(), ControllerError<B>> {
        let mut subscriptions = self.inner.subscriptions.write().await;

        if let Some(subscription) = subscriptions.get_mut(&subscription_id) {
            subscription
                .replay(checkpoint.clone())
                .await
                .map_err(ControllerError::Subscription)?;
        }

        Ok(())
    }

    pub async fn unsubscribe(
        &self,
        subscription_id: SubscriptionId,
    ) -> Result<(), ControllerError<B>> {
        let mut subscriptions = self.inner.subscriptions.write().await;

        if let Some(subscription) = subscriptions.remove(&subscription_id) {
            subscription
                .unsubscribe()
                .await
                .map_err(ControllerError::Subscription)?;
        }

        Ok(())
    }

    pub async fn commit(&self, operation_id: Hash) -> Result<(), ControllerError<B>> {
        let mut checkpoints = self.inner.checkpoints.write().await;
        checkpoints.insert(operation_id);
        Ok(())
    }

    async fn get_or_create_checkpoint(&self, subject: &Subject) -> Checkpoint {
        // @TODO: Properly compute checkpoint from looking into operations store.
        Checkpoint::default()
    }
}

#[derive(Debug, Error)]
pub enum ControllerError<B>
where
    B: Backend,
{
    #[error("{0}")]
    Backend(<B as Backend>::Error),

    #[error("{0}")]
    Subscription(<B::Subscription as Subscription>::Error),
}
