use std::{
    convert::Infallible,
    error::Error,
    pin::Pin,
    task::{Context, Poll},
};

use futures_core::Stream;
use futures_util::{Sink, SinkExt, StreamExt};
use pin_project::pin_project;

pub trait Transport<SinkItem, Item>
where
    Self: Stream<Item = Result<Item, <Self as Sink<SinkItem>>::Error>>,
    Self: Sink<SinkItem, Error = <Self as Transport<SinkItem, Item>>::Error>,
    <Self as Sink<SinkItem>>::Error: Error,
{
    type Error: Error + Send + Sync + 'static;
}

impl<T, SinkItem, Item, E> Transport<SinkItem, Item> for T
where
    T: ?Sized,
    T: Stream<Item = Result<Item, E>>,
    T: Sink<SinkItem, Error = E>,
    T::Error: Error + Send + Sync + 'static,
{
    type Error = E;
}

pub type Query = (); // dummy

pub type Checkpoint = (); // dummy

pub type SubscriptionId = u64;

pub type TopicId = [u8; 32];

pub enum Request {
    Publish {
        // @TODO: If we don't mention the log id how can a shared node route the operation
        // correctly?
        header: Vec<u8>,
        body: Vec<u8>,
    },
    Subscribe {
        query: Query,
        from: Checkpoint,
        live: bool,
    },
    Replay {
        id: SubscriptionId,
        from: Checkpoint,
    },
    Unsubscribe {
        id: SubscriptionId,
    },
    SubscribeEphemeral {
        topic_id: TopicId,
    },
    UnsubscribeEphemeral {
        topic_id: TopicId,
    },
}

pub enum Response {
    SubscriptionStarted {
        id: SubscriptionId,
    },
    SubscriptionEnded {
        id: SubscriptionId,
    },
    Operation {
        id: SubscriptionId,
        header: Vec<u8>,
        body: Vec<u8>,
    },
    EphemeralSubscriptionStarted {
        id: SubscriptionId,
    },
    EphemeralSubscriptionEnded {
        id: SubscriptionId,
    },
    EphemeralMessage {
        id: SubscriptionId,
        payload: Vec<u8>,
    },
}

pub struct Controller<T>
where
    T: Transport<Request, Response>,
{
    backend: T,
}

impl<T> Controller<T>
where
    T: Transport<Request, Response>,
{
    pub async fn publish(
        &self,
        header: Vec<u8>,
        body: Vec<u8>,
    ) -> Result<(), <T as Transport<Request, Response>>::Error> {
        Ok(())
    }
}
