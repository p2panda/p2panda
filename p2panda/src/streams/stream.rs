// SPDX-License-Identifier: MIT OR Apache-2.0

use std::marker::PhantomData;
use std::pin::Pin;
use std::task::{Context, Poll};

use futures_util::Stream;
use p2panda_core::{Hash, PublicKey};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{Header, Topic};

/// Handle onto an eventually-consistent stream, exposes API for publishing messages, subscribing
/// to the event stream, and committing received messages.
pub struct StreamHandle<M> {
    topic: Topic,
    _marker: PhantomData<M>,
}

impl<M> StreamHandle<M>
where
    M: Serialize + for<'a> Deserialize<'a>,
{
    pub fn topic(&self) -> Topic {
        self.topic
    }

    /// Publish a message.
    pub async fn publish(&self, _message: M) -> Result<Hash, StreamError> {
        unimplemented!()
    }

    /// Subscribe to the message stream.
    pub async fn subscribe(&self) -> Result<StreamSubscription<M>, StreamError> {
        unimplemented!()
    }

    /// Manually commit a message.
    // TODO: Implementing this is not a priority right now.
    pub async fn commit(&self, _message_id: Hash) -> Result<(), StreamError> {
        // This is a no-op if messages are automatically committed (which is the default).
        unimplemented!()
    }

    /// Repeat streaming all known messages again.
    ///
    /// This can be useful if the application doesn't keep any materialised state around and needs
    /// to repeat all messages on start.
    ///
    /// Another use-case is the roll-out of an application update where all state needs to be
    /// re-materialised.
    // TODO: This will be required by applications like Reflection.
    pub async fn replay(&self) -> Result<(), StreamError> {
        unimplemented!()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum StreamEvent<M> {
    // TODO: Add more topic-related system events here which can come from node.
    Message(Message<M>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Message<M> {
    header: Header,
    topic: Topic,
    body: M,
}

impl<M> Message<M> {
    pub fn topic(&self) -> Topic {
        self.topic
    }

    pub fn id(&self) -> Hash {
        self.header.hash()
    }

    pub fn author(&self) -> PublicKey {
        self.header.public_key
    }

    pub fn timestamp(&self) -> u64 {
        self.header.timestamp
    }

    pub fn header(&self) -> &Header {
        &self.header
    }

    pub fn body(&self) -> &M {
        &self.body
    }

    pub fn commit(&self) {
        unimplemented!()
    }
}

/// Subscription to events arriving from a stream.
pub struct StreamSubscription<M> {
    _marker: PhantomData<M>,
}

impl<M> StreamSubscription<M>
where
    M: Serialize + for<'a> Deserialize<'a>,
{
    pub fn topic(&self) -> Topic {
        unimplemented!()
    }
}

impl<M> Stream for StreamSubscription<M>
where
    M: Serialize + for<'a> Deserialize<'a>,
{
    type Item = Result<StreamEvent<M>, StreamError>;

    fn poll_next(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        unimplemented!()
    }
}

#[derive(Debug, Error)]
pub enum StreamError {}
