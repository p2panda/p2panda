// SPDX-License-Identifier: MIT OR Apache-2.0

use std::marker::PhantomData;
use std::pin::Pin;
use std::task::{Context, Poll};

use futures_core::Stream;
use p2panda_core::{PublicKey, Signature};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::Topic;

/// Handle onto an ephemeral stream, exposes API for publishing messages and subscribing to the
/// event stream.
pub struct EphemeralStreamHandle<M> {
    topic: Topic,
    _marker: PhantomData<M>,
}

impl<M> EphemeralStreamHandle<M>
where
    M: Serialize + for<'a> Deserialize<'a>,
{
    pub fn topic(&self) -> Topic {
        unimplemented!()
    }

    pub async fn publish(&self, _message: M) -> Result<(), EphemeralStreamError> {
        unimplemented!()
    }

    pub async fn subscribe(&self) -> Result<EphemeralStreamSubscription<M>, EphemeralStreamError> {
        unimplemented!()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EphemeralStreamEvent<M> {
    Message(EphemeralMessage<M>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EphemeralMessage<M> {
    topic: Topic,
    public_key: PublicKey,
    signature: Signature,
    timestamp: u64,
    body: M,
}

impl<M> EphemeralMessage<M> {
    pub fn topic(&self) -> Topic {
        self.topic
    }

    pub fn author(&self) -> PublicKey {
        self.public_key
    }

    pub fn timestamp(&self) -> u64 {
        self.timestamp
    }

    pub fn body(&self) -> &M {
        &self.body
    }
}

pub struct EphemeralStreamSubscription<M> {
    _marker: PhantomData<M>,
}

impl<M> EphemeralStreamSubscription<M>
where
    M: Serialize + for<'a> Deserialize<'a>,
{
    pub fn topic(&self) -> Topic {
        unimplemented!()
    }
}

impl<M> Stream for EphemeralStreamSubscription<M>
where
    M: Serialize + for<'a> Deserialize<'a>,
{
    type Item = EphemeralStreamEvent<M>;

    fn poll_next(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        todo!()
    }
}

#[derive(Debug, Error)]
pub enum EphemeralStreamError {}
