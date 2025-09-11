// SPDX-License-Identifier: MIT OR Apache-2.0

#![allow(dead_code, unused)]
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::{error::Error, marker::PhantomData};

use futures_core::Stream;
use futures_util::Sink;
use p2panda_core::{Hash, Operation, PrivateKey};

use crate::{Checkpoint, Query};

pub type TopicId = [u8; 32];

pub struct ClientBuilder {
    private_key: Option<PrivateKey>,
}

impl ClientBuilder {
    pub fn new() -> Self {
        Self { private_key: None }
    }

    pub fn private_key(mut self, private_key: PrivateKey) -> Self {
        self.private_key = Some(private_key);
        self
    }

    pub fn build(self) -> Client<(), ()> {
        Client {
            private_key: self.private_key.unwrap_or(PrivateKey::new()),
            stream_controller: StreamController::new((), ()),
        }
    }
}

pub struct Client<S, B> {
    private_key: PrivateKey,
    stream_controller: StreamController<S, B>,
}

impl<S, B> Client<S, B> {
    pub fn create_query<M>(&self) -> QueryHandle<M, S, B> {
        // @TODO: Create operation and derive hash from there?

        let root = Hash::new(b"todo");
        let query = Query::from_hash(root);

        QueryHandle {
            query,
            stream_controller: self.stream_controller.clone(),
            _marker: PhantomData,
        }
    }

    pub fn query<M>(&self, query: Query) -> QueryHandle<M, S, B> {
        QueryHandle {
            query,
            stream_controller: self.stream_controller.clone(),
            _marker: PhantomData,
        }
    }

    // @TODO: Is M following the same design in `p2panda-spaces`?
    pub fn create_space<M>(&self) -> SpaceHandle<M> {
        todo!()
    }

    pub fn space<M>(&self, space_id: Hash) -> SpaceHandle<M> {
        todo!()
    }

    pub fn ephemeral<M>(&self, topic_id: TopicId) -> EphemeralHandle<M> {
        EphemeralHandle {
            topic_id,
            _marker: PhantomData,
        }
    }
}

// Query

pub struct QueryHandle<M, S, B> {
    query: Query,
    stream_controller: StreamController<S, B>,
    _marker: PhantomData<M>,
}

impl<M, S, B> QueryHandle<M, S, B> {
    pub fn filter(&self, value: &str) -> Self {
        QueryHandle {
            query: self.query.clone().with_suffix(value),
            stream_controller: self.stream_controller.clone(),
            _marker: PhantomData,
        }
    }

    pub fn publish(&self, message: M) {
        // @TODO: Call forge
        todo!()
    }

    pub fn subscribe(&self) -> QuerySubscription<M> {
        let subscription = self.stream_controller.subscribe(self.query.clone());

        // @TODO: Route subscribed stream events into processors (orderer)

        QuerySubscription {
            _marker: PhantomData,
        }
    }

    pub fn subscribe_from(&self, from: Checkpoint, live: bool) -> QuerySubscription<M> {
        let subscription = self.stream_controller.subscribe(self.query.clone());

        // @TODO: Route subscribed stream events into processors (orderer)

        QuerySubscription {
            _marker: PhantomData,
        }
    }
}

pub struct QuerySubscription<M> {
    _marker: PhantomData<M>,
}

impl<M> Stream for QuerySubscription<M> {
    type Item = M;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        todo!()
    }
}

// Space

pub struct SpaceHandle<M> {
    space_id: Hash,
    _marker: PhantomData<M>,
}

impl<M> SpaceHandle<M> {
    pub fn publish(&self, message: M) {
        todo!()
    }

    pub fn subscribe(&self) -> SpaceSubscription<M> {
        // let subscription = self.stream_controller.subscribe(self.space_id.into());

        SpaceSubscription {
            _marker: PhantomData,
        }
    }
}

pub struct SpaceSubscription<M> {
    _marker: PhantomData<M>,
}

// Ephemeral

pub struct EphemeralHandle<M> {
    topic_id: TopicId,
    _marker: PhantomData<M>,
}

impl<M> EphemeralHandle<M> {
    pub fn publish(&self, message: M) {
        todo!()
    }

    pub fn subscribe(&self) -> EphemeralSubscription<M> {
        EphemeralSubscription {
            _marker: PhantomData,
        }
    }
}

pub struct EphemeralSubscription<M> {
    _marker: PhantomData<M>,
}

// Controller

pub struct StreamController<S, B> {
    inner: Arc<StreamControllerInner<S, B>>,
}

impl<S, B> Clone for StreamController<S, B> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

struct StreamControllerInner<S, B> {
    api: B,
    store: S,
}

impl<S, B> StreamController<S, B>
where
    B: Transport<Request, Response>,
{
    pub(crate) fn new(store: S, api: B) -> Self {
        let inner = StreamControllerInner { api, store };

        Self {
            inner: Arc::new(inner),
        }
    }

    pub(crate) fn publish(&self, operation: Operation<()>) {
        todo!()
    }

    pub(crate) fn subscribe(&self, query: Query) {
        // @TODO
        // 1. Check if it already exists, return, otherwise create new subscription
        // 2. Look up last known checkpoint in store
        // 3. Subscribe at backend with the given arguments (query, checkpoint, livemode=true)
        // 4. Forward stream

        // let checkpoint = Checkpoint::new();
        // let rx = self.api.subscribe(query, checkpoint, true)?;

        todo!()
    }

    pub(crate) fn subscribe_ephemeral(&self, topic_id: TopicId) {
        todo!()
    }

    pub(crate) fn commit(&self, operation_id: Hash) {
        todo!()
    }
}

enum Request {
    Publish {
        operation: Operation<()>,
    },
    Subscribe {
        query: Query,
        from: Checkpoint,
        live: bool,
    },
    SubscribeEphemeral {
        topic_id: TopicId,
    },
    SubscribeSystem,
}

enum Response {}

pub trait Transport<SinkItem, Item>
where
    Self: Stream<Item = Result<Item, <Self as Sink<SinkItem>>::Error>>,
    Self: Sink<SinkItem, Error = <Self as Transport<SinkItem, Item>>::Error>,
    <Self as Sink<SinkItem>>::Error: Error,
{
    /// Associated type where clauses are not elaborated; this associated type allows users
    /// bounding types by Transport to avoid having to explicitly add `T::Error: Error` to their
    /// bounds.
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

#[cfg(test)]
mod tests {
    use futures_util::{StreamExt, task};
    use p2panda_core::Hash;

    use crate::client::ClientBuilder;

    #[tokio::test]
    async fn it_works() {
        enum Message {
            Lala,
        }

        enum MessageBla {}

        let client = ClientBuilder::new().build();

        let handle = client.query::<Message>(Hash::new(b"test").into());

        let another_handle = handle.filter("test");

        let rx = another_handle.subscribe();

        tokio::spawn(async {
            while let Some(event) = rx.next().await {
                break;
            }

            rx.replay().await;

            while let Some(event) = rx.next().await {
                break;
            }
        });

        handle.publish(Message::Lala);

        let handle_bla = client.query::<MessageBla>("/test/lala/bla".try_into().unwrap());
    }
}
